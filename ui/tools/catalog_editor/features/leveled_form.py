"""LeveledLists on the workbench: the roll preview ("What drops"), the situation bar and the inspector tabs.

The odds come from ``simulate`` (a port of Tales' roll); without it the table still lists every item, just
without odds.
"""

from __future__ import annotations

import copy
import logging
from collections.abc import Mapping
from dataclasses import dataclass, field
from typing import Any

from ui.tools.catalog_editor.features import EntryView

_log = logging.getLogger("tools.catalog_editor")

# (bit, label, what it does). Bit meanings are Tales' LeveledListCatalog.h `flags::`.
FLAG_HELP: tuple[tuple[int, str, str], ...] = (
    (0x01, "Any item level", "Consider items of every level, not only those at or below the roll level."),
    (0x02, "Roll once per count", "The list is rolled again for each unit of count, so each unit can be a different item."),
    (0x04, "Give every item", "Give every item that passes its checks instead of picking one."),
    (0x08, "Disable the actor if empty", "An actor whose own template roll comes back empty is disabled (only for NPC lists)."),
    (0x40, "First passing item wins", "Walk the items in order and take the first one whose checks pass."),
    (0x80, "Step up a level band", "When the roll lands on a level band with nothing passing, move up to the next band."),
    (0x200, "Weighted", "Each item's 'chance of nothing' becomes its weight: a bigger number is picked more often."),
)
IGNORED_FLAG_HELP: tuple[tuple[int, str, str], ...] = (
    (0x10, "Dispenser", "Fallout 76 marker for per-player pickups. Tales ignores it."),
    (0x20, "Hide marker", "Fallout 76 map-marker option. Tales ignores it."),
    (0x100, "Workshop list", "Fallout 76 workshop option. Tales ignores it."),
)
SCOPE_LABELS = {"seventysix": "Only in the SeventySix world", "always": "Everywhere"}
HOURS = ("Any time (not set)", "Day, 12:00", "Night, 23:00")
HOUR_VALUES = (None, 12, 23)
ASSUME = ("ask", "assume they pass", "assume they fail")
ROLLS = 1000

_COLUMN_HELP = {
    "level": "The lowest roll level at which this item can be picked. Raiders roll at their own level. This is not a "
             "check on the player: add \"the player's level is at least...\" for that.",
    "count": "How many of the item are given.",
    "chance_none": "Percent chance this item gives nothing when picked (its weight when the list is 'Weighted').",
}


def set_flag(flags: int, bit: int, on: bool) -> int:
    return flags | bit if on else flags & ~bit


def normalized_extra(extra: dict | None) -> dict | None:
    """Drops what the owner kind makes meaningless: an owner-less extra keeps only health, rank beats global."""
    if not extra:
        return None
    out = {"health": extra.get("health", 1.0)}
    owner = extra.get("owner")
    if owner:
        out["owner"] = owner
        if extra.get("rank") is not None:
            out["rank"] = int(extra["rank"])
        elif extra.get("global"):
            out["global"] = extra["global"]
    return out


def owner_kind(extra: dict | None) -> str:
    if not extra or not extra.get("owner"):
        return "none"
    return "faction" if extra.get("rank") is not None else "other"


def _number(row: dict | None) -> float:
    return float((row or {}).get("value", 0) or 0)


def plain_summary(row: dict, name_of=lambda key: key) -> str:
    """One paragraph: what rolling this list does in game."""
    flags = int(row.get("flags", 0))
    entries = row.get("entries", [])
    count = len(entries)
    noun = "items" if count != 1 else "item"
    where = {"seventysix": "In the SeventySix world only, this", "always": "Everywhere, this"}.get(row.get("scope"), "This")
    kind = "creature list" if row.get("signature") == "LVLN" else "loot list"
    first = f"{where} {kind} changes how {name_of(row.get('target', '')) or 'the list'} rolls."
    sentences = [first]
    if count == 0:
        sentences.append("It has no items yet, so it gives nothing.")
    elif flags & 0x04:
        sentences.append(f"It gives every one of its {count} {noun} that passes its checks.")
    elif flags & 0x40:
        sentences.append(f"It takes the first of its {count} {noun}, in order, that passes its checks.")
    elif flags & 0x200:
        sentences.append(f"It picks one of its {count} {noun}, favouring those with a larger weight.")
    else:
        sentences.append(f"It picks one of its {count} {noun} that passes its checks, all equally likely.")
    chance = row.get("chance_none") or {}
    if chance.get("global"):
        sentences.append("The chance of getting nothing comes from a global value.")
    elif _number(chance):
        sentences.append(f"There is a {_number(chance):g}% chance of getting nothing.")
    maximum = _number(row.get("max_count"))
    if maximum:
        sentences.append(f"At most {maximum:g} of any one item are given.")
    if flags & 0x02:
        sentences.append("Each unit of an item's count is rolled separately.")
    if flags & 0x01:
        sentences.append("Items of every level are eligible.")
    if flags & 0x80:
        sentences.append("When nothing passes at the rolled level band, the next one up is tried.")
    checked = sum(bool(entry.get("conditions")) for entry in entries)
    if checked:
        sentences.append(f"{checked} {'items have' if checked != 1 else 'item has'} its own checks.")
    if row.get("conditions"):
        sentences.append("The whole list only applies when its list checks pass; otherwise Fallout 4 rolls it.")
    return " ".join(sentences)


def item_sentence(item: dict, name_of=lambda key: key, weighted: bool = False) -> str:
    """'Give Pipe Rifle x1 from level 15, with a 25% chance of nothing, only when it is night.'"""
    from ui.tools.catalog_editor.widgets import templates

    form = item.get("form", "")
    text = f"Give {name_of(form) if form else '(no item picked)'} x{_number(item.get('count')):g}"
    text += f" from level {_number(item.get('level')):g}"
    chance = _number(item.get("chance_none"))
    if weighted:
        text += f", with weight {chance:g}"
    elif chance:
        text += f", with a {chance:g}% chance of nothing"
    checks = item.get("conditions") or []
    if checks:
        joiner = " or " if templates.chain_mode(checks) == "any" else " and "
        words = [templates.sentence(check, name_of) for check in checks]
        text += ", only when " + joiner.join(word[:1].lower() + word[1:] for word in words)
    return text + "."


def new_item() -> dict:
    return {"form": "", "level": {"value": 1}, "count": {"value": 1}, "chance_none": {"value": 0}, "conditions": []}


# --- the situation ---------------------------------------------------------------------------------


@dataclass
class SituationState:
    level: int = 25
    in_seventysix: bool = True
    region: str = ""
    hour: int = 0  # index into HOURS
    has_subject: bool = True
    assume: int = 0  # index into ASSUME
    unknown_count: int = 0
    histogram: tuple[Any, dict[str, int]] | None = None

    def situation(self, view: EntryView, entry: dict, document: dict, unknown_keys: list[str] = ()):
        from ui.tools.catalog_editor import simulate

        names = {}
        for item in entry.get("entries") or []:
            if item.get("form"):
                names[item["form"]] = view.name_of(item["form"])
        if self.region:
            names[self.region] = view.name_of(self.region)
        assume = {key: self.assume == 1 for key in unknown_keys} if self.assume else {}
        globals_ = GlobalValues(view.forms) if hasattr(view.forms, "global_value") else {}
        return simulate.Situation(level=self.level, in_seventysix=self.in_seventysix, region=self.region or None,
                                  hour=HOUR_VALUES[self.hour], has_subject=self.has_subject, assume=assume, names=names,
                                  globals=globals_, document=document)

    def signature(self) -> tuple:
        return (self.level, self.in_seventysix, self.region, self.hour, self.has_subject, self.assume)


class GlobalValues(Mapping):
    """Situation.globals read from the loaded plugins on first use; a list can name hundreds of globals."""

    def __init__(self, forms):
        self._forms = forms
        self._seen: set[str] = set()

    def __getitem__(self, key: str) -> float:
        value = self._forms.global_value(key)
        if value is None:
            raise KeyError(key)
        self._seen.add(key)
        return value

    def __contains__(self, key) -> bool:
        return isinstance(key, str) and self.get(key) is not None

    def __iter__(self):
        return iter(self._seen)

    def __len__(self) -> int:
        return len(self._seen)


@dataclass
class PreviewCache:
    key: Any = None
    preview: Any = None
    note: str = ""
    unknown_keys: list[str] = field(default_factory=list)


def run_preview(entry: dict, situation, resolve_list) -> tuple[Any, str]:
    """(ListPreview, "") or (None, why the odds are unavailable); the table works either way."""
    from ui.tools.catalog_editor import simulate

    try:
        return simulate.preview_list(entry, situation, resolve_list), ""
    except NotImplementedError:
        return None, "odds unavailable: the roll preview isn't built yet"
    except Exception as error:
        _log.exception("roll preview failed")
        return None, f"odds unavailable: the preview failed ({error})"


def run_sample(entry: dict, situation, resolve_list) -> tuple[dict[str, int] | None, str]:
    from ui.tools.catalog_editor import simulate

    try:
        return simulate.sample(entry, situation, ROLLS, 0, resolve_list), ""
    except NotImplementedError:
        return None, "rolling isn't available yet"
    except Exception as error:
        _log.exception("roll sample failed")
        return None, f"rolling failed ({error})"


# --- drawing helpers -------------------------------------------------------------------------------


def _props(schema: dict, section_item: dict) -> dict:
    from ui.tools.catalog_editor import render

    return render._resolve(schema, section_item).get("properties", {})


def _help(text: str) -> None:
    from imgui_bundle import imgui

    imgui.same_line()
    imgui.text_disabled("(?)")
    if imgui.is_item_hovered():
        imgui.set_tooltip(text)


def _heading(text: str, help_text: str) -> None:
    from imgui_bundle import imgui

    imgui.separator_text(text)
    _help(help_text)


def _field(label: str, node: dict, entry: dict, key: str, ctx, path: str, help_text: str) -> bool:
    from ui.tools.catalog_editor import render

    changed, new = render.draw_value(label, node, copy.deepcopy(entry.get(key)), ctx, f"{path}/{key}")
    if changed:
        entry[key] = new
    _help(help_text)
    return changed


def _draw_flags(entry: dict) -> bool:
    from imgui_bundle import imgui

    flags = int(entry.get("flags", 0))
    changed = False
    for bit, label, text in FLAG_HELP:
        ticked, on = imgui.checkbox(f"{label}##flag{bit}", bool(flags & bit))
        if imgui.is_item_hovered():
            imgui.set_tooltip(text)
        if ticked:
            flags, changed = set_flag(flags, bit, on), True
    ignored = flags & sum(bit for bit, _, _ in IGNORED_FLAG_HELP)
    if ignored:
        names = ", ".join(label for bit, label, _ in IGNORED_FLAG_HELP if flags & bit)
        imgui.text_disabled(f"Also set, ignored by Tales: {names}")
    if changed:
        entry["flags"] = flags
    return changed


def _draw_extra(item: dict, ctx, path: str) -> bool:
    from imgui_bundle import imgui

    from ui.tools.catalog_editor import render

    extra = item.get("extra")
    kind = owner_kind(extra)
    labels = ["Nobody", "A faction, at a rank", "An NPC or a faction, needing a global"]
    index = {"none": 0, "faction": 1, "other": 2}[kind]
    imgui.text("Owned by")
    _help("Who owns the item when it is handed out. Owned items count as stolen when someone else takes them.")
    imgui.set_next_item_width(260)
    picked, index = imgui.combo(f"##owner_kind{path}", index, labels)
    changed = False
    if picked:
        current = dict(extra or {})
        if index == 0:
            current = {"health": current["health"]} if "health" in current else {}
        elif index == 1:
            current.setdefault("owner", "")
            current["rank"] = int(current.get("rank") or 0)
            current.pop("global", None)
        else:
            current.setdefault("owner", "")
            current.pop("rank", None)
        item["extra"] = current or None
        extra, changed = item["extra"], True
    if index != 0:
        extra = dict(item.get("extra") or {})
        owner_node = {"type": "string", "pattern": "^[^\\s:]+\\.[^\\s:.]+:[0-9A-Fa-f]{1,8}$",
                      "x-form-type": ["FACT"] if index == 1 else ["NPC_", "FACT"]}
        owned, new_owner = render.draw_value("Owner", owner_node, extra.get("owner", ""), ctx, f"{path}/owner")
        _help("The faction (or NPC) that owns the item.")
        if owned:
            extra["owner"], changed = new_owner, True
        if index == 1:
            imgui.set_next_item_width(120)
            moved, rank = imgui.input_int(f"Rank##rank{path}", int(extra.get("rank") or 0))
            _help("Faction rank needed to take the item without it counting as stealing.")
            if moved:
                extra["rank"], changed = rank, True
        else:
            global_node = {"type": "string", "pattern": "^[^\\s:]+\\.[^\\s:.]+:[0-9A-Fa-f]{1,8}$", "x-form-type": ["GLOB"]}
            glob, new_global = render.draw_value("Required global", global_node, extra.get("global", ""), ctx,
                                                 f"{path}/global")
            _help("A global value that the owner's ownership check reads.")
            if glob:
                extra["global"], changed = new_global, True
        item["extra"] = extra
    extra = item.get("extra")
    if extra is not None or index != 0:
        imgui.set_next_item_width(260)
        health = float((extra or {}).get("health", 1.0))
        moved, health = imgui.slider_float(f"Item condition##health{path}", health, 0.0, 1.0, "%.2f")
        _help("Condition of the handed-out item as a fraction of full health (1.00 is brand new).")
        if moved:
            item["extra"] = {**(extra or {}), "health": health}
            changed = True
    elif imgui.small_button(f"Set item condition...##addhealth{path}"):
        item["extra"], changed = {"health": 1.0}, True
    if changed:
        item["extra"] = normalized_extra(item.get("extra"))
        if item["extra"] is None:
            item.pop("extra", None)
    return changed


def _strike() -> None:
    from imgui_bundle import imgui

    low, high = imgui.get_item_rect_min(), imgui.get_item_rect_max()
    middle = (low.y + high.y) / 2
    imgui.get_window_draw_list().add_line(imgui.ImVec2(low.x, middle), imgui.ImVec2(high.x, middle),
                                          imgui.get_color_u32(imgui.Col_.text_disabled), 1.5)


def _odds_cell(odds: float | None) -> None:
    from imgui_bundle import imgui

    if odds is None:
        imgui.text_disabled("-")
        return
    imgui.progress_bar(max(0.0, min(1.0, odds)), imgui.ImVec2(-1, 0), f"{odds * 100:.1f}%")


# --- the views -------------------------------------------------------------------------------------


def draw_situation(ext, view: EntryView) -> None:
    from imgui_bundle import imgui

    state = ext.situation
    imgui.set_next_item_width(160)
    _, state.level = imgui.slider_int("Roll level", state.level, 1, 100)
    imgui.set_item_tooltip("The level the list is rolled at: an actor's own level, or the container's location level.")
    imgui.same_line()
    _, state.in_seventysix = imgui.checkbox("In the SeventySix world", state.in_seventysix)
    imgui.set_item_tooltip("Lists scoped to the SeventySix world are left to Fallout 4 elsewhere.")
    imgui.same_line()
    regions = sorted(view.doc.section_keys("regions")) if "regions" in view.doc.sections() else []
    imgui.set_next_item_width(200)
    if imgui.begin_combo("Region", view.name_of(state.region) if state.region else "anywhere"):
        if imgui.selectable("anywhere", not state.region)[0]:
            state.region = ""
        for key in regions:
            if imgui.selectable(f"{view.name_of(key)}##{key}", key == state.region)[0]:
                state.region = key
        imgui.end_combo()
    imgui.set_next_item_width(150)
    _, state.hour = imgui.combo("When", state.hour, list(HOURS))
    imgui.same_line()
    _, state.has_subject = imgui.checkbox("Rolled for a container or actor", state.has_subject)
    imgui.set_item_tooltip("When a roll has no subject, the list's checks are skipped. Item lists see the player as the target.")
    imgui.same_line()
    imgui.set_next_item_width(200)
    labels = [f"ask ({state.unknown_count})", *ASSUME[1:]]
    _, state.assume = imgui.combo("Unknowns", state.assume, labels)
    imgui.set_item_tooltip("Checks that can't be worked out offline (weather keywords, ESS-rolled location keywords, quest "
                           "state). 'ask' leaves them unanswered; the others assume every one passes or fails.")
    imgui.same_line()
    if imgui.button(f"Roll {ROLLS:,}x"):
        ext.roll(view)


def _pairs(view: EntryView) -> list[tuple[int | None, int | None, str]]:
    """(master index, mine index, kind) of the list's items, matched by form then position."""
    from ui.tools.catalog_editor.model import OVERRIDDEN
    from ui.tools.catalog_editor.workbench import diff_entry

    entries = view.entry.get("entries") or []
    if view.row.status != OVERRIDDEN or not isinstance(view.row.inherited, dict):
        return [(None, index, "same") for index in range(len(entries))]
    lines = [line for line in diff_entry({"entries": view.row.inherited.get("entries") or []}, {"entries": entries})]
    return [(line.master_index, line.mine_index, line.kind) for line in lines]


def draw_drops(ext, view: EntryView) -> tuple[bool, Any]:
    from imgui_bundle import imgui

    from creation_lib.ui.widgets.modern import semantic_color
    from ui.tools.catalog_editor.model import ADDED, REMOVED
    from ui.tools.catalog_editor.workbench import diff_entry, revert_line

    entry = view.entry if isinstance(view.entry, dict) else {}
    plugin = view.doc.plugin
    imgui.text_disabled(ext.roll_line(entry))
    if view.row.status == REMOVED:
        imgui.text_wrapped("Sent to Fallout 4's roll: Tales doesn't roll this list, so Fallout 4 rolls the plugin's own "
                           "record. Below is what the master's list held.")
    preview, note = ext.preview(view)
    if preview is not None:
        if not preview.used_by_tales:
            imgui.text_colored(semantic_color("muted"), "List not used here: Fallout 4 rolls it in this situation.")
        if preview.falls_back:
            imgui.text_colored(semantic_color("error"), "This list falls back: the odds are Fallout 4's, not Tales'.")
        for line in preview.notes:
            imgui.text_disabled(line)
    else:
        imgui.text_disabled(note)
    outcomes = {outcome.index: outcome for outcome in preview.entries} if preview is not None else {}
    _, ext.only_eligible = imgui.checkbox("Show only items that can drop", ext.only_eligible)
    new_entry = None
    if not view.read_only:
        imgui.same_line()
        if imgui.button("+ Add item"):
            new_entry = copy.deepcopy(entry)
            new_entry.setdefault("entries", []).append(new_item())
            view.select_item(len(new_entry["entries"]) - 1)
            view.focus_tab("Item")
    master_entries = (view.row.inherited or {}).get("entries") or [] if isinstance(view.row.inherited, dict) else []
    entries = entry.get("entries") or []
    weighted = bool(int(entry.get("flags", 0)) & 0x200)
    flags = (imgui.TableFlags_.row_bg | imgui.TableFlags_.borders_inner_h | imgui.TableFlags_.resizable
             | imgui.TableFlags_.sizing_stretch_prop)
    if imgui.begin_table("##drops", 7, flags):
        imgui.table_setup_column("Item", imgui.TableColumnFlags_.width_stretch, 3.0)
        imgui.table_setup_column("Odds", imgui.TableColumnFlags_.width_stretch, 1.4)
        imgui.table_setup_column("Count", imgui.TableColumnFlags_.width_stretch, 0.6)
        imgui.table_setup_column("From lvl", imgui.TableColumnFlags_.width_stretch, 0.7)
        imgui.table_setup_column("Why / why not", imgui.TableColumnFlags_.width_stretch, 3.5)
        imgui.table_setup_column("Changed by", imgui.TableColumnFlags_.width_stretch, 1.4)
        imgui.table_setup_column("##actions", imgui.TableColumnFlags_.width_fixed, imgui.calc_text_size("Restore__").x)
        imgui.table_headers_row()
        for master_index, mine_index, kind in _pairs(view):
            removed = mine_index is None
            item = master_entries[master_index] if removed else entries[mine_index]
            outcome = outcomes.get(mine_index) if not removed else None
            if ext.only_eligible and (removed or (outcome is not None and not outcome.eligible)):
                continue
            imgui.push_id(f"{master_index}/{mine_index}")
            imgui.table_next_row()
            imgui.table_next_column()
            name = view.name_of(item.get("form", "")) or "(no item picked)"
            if removed:
                imgui.text_disabled(name)
                _strike()
            elif imgui.selectable(name, view.item == mine_index,
                                  imgui.SelectableFlags_.span_all_columns | imgui.SelectableFlags_.allow_overlap)[0]:
                view.select_item(mine_index)
            if not removed:
                imgui.set_item_tooltip(item_sentence(item, view.name_of, weighted))
            imgui.table_next_column()
            _odds_cell(outcome.odds if outcome is not None else None)
            imgui.table_next_column()
            imgui.text(f"{_number(item.get('count')):g}")
            imgui.table_next_column()
            imgui.text(f"{_number(item.get('level')):g}")
            imgui.table_next_column()
            if removed:
                imgui.text_disabled("removed by you")
            elif outcome is not None:
                why = "; ".join(outcome.reasons) or ("can drop" if outcome.eligible else "can't drop")
                imgui.text_colored(semantic_color("text" if outcome.eligible else "muted"), why)
            else:
                checks = len(item.get("conditions") or [])
                imgui.text_disabled(f"{checks} check{'s' if checks != 1 else ''}" if checks else "no checks")
            imgui.table_next_column()
            source = plugin if (removed or kind != "same" or view.row.status == ADDED) else view.master
            imgui.text_disabled(source + {"changed": " (changed)", "added": " (added)"}.get(kind, ""))
            imgui.table_next_column()
            if removed and not view.read_only:
                if imgui.small_button("Restore"):
                    line = next(line for line in diff_entry({"entries": master_entries}, {"entries": entries})
                                if line.master_index == master_index and line.mine_index is None)
                    new_entry = revert_line(entry, {"entries": master_entries}, line)
                warning = ext.restore_warning(view, master_entries, entries, master_index)
                if warning:
                    imgui.set_item_tooltip(warning)
            elif not removed and not view.read_only and imgui.small_button("Remove"):
                new_entry = copy.deepcopy(entry)
                del new_entry["entries"][mine_index]
                view.select_item(None)
            imgui.pop_id()
        imgui.table_next_row()
        imgui.table_next_column()
        imgui.text_disabled("(nothing)")
        imgui.table_next_column()
        _odds_cell(preview.chance_none if preview is not None else None)
        for _ in range(2):
            imgui.table_next_column()
        imgui.table_next_column()
        chance = entry.get("chance_none") or {}
        imgui.text_disabled("list chance of nothing" + (" (from a global)" if chance.get("global") else
                                                         f": {_number(chance):g}%"))
        imgui.table_next_column()
        before = (view.row.inherited or {}).get("chance_none") if isinstance(view.row.inherited, dict) else None
        imgui.text_disabled(view.master if view.master and before == entry.get("chance_none") else plugin)
        imgui.end_table()
    if ext.situation.histogram is not None:
        _draw_histogram(ext, view)
    return new_entry is not None, new_entry if new_entry is not None else entry


def _draw_histogram(ext, view: EntryView) -> None:
    from imgui_bundle import imgui

    key, counts = ext.situation.histogram
    if key != ext.histogram_key(view):
        ext.situation.histogram = None
        return
    imgui.separator_text(f"{ROLLS:,} rolls at these settings")
    if not counts:
        imgui.text_disabled("Nothing rolled: Fallout 4 rolls this list in this situation.")
        return
    width = max(imgui.calc_text_size(view.name_of(form) or "nothing").x for form in counts)
    for form, count in sorted(counts.items(), key=lambda pair: -pair[1]):
        imgui.text(view.name_of(form) if form else "nothing")
        imgui.same_line(width + imgui.get_style().item_spacing.x * 2)
        imgui.progress_bar(count / ROLLS, imgui.ImVec2(-1, 0), f"{count} ({count * 100 / ROLLS:.1f}%)")


def _selected_item(view: EntryView) -> tuple[dict, list, dict | None]:
    entry = copy.deepcopy(view.entry) if isinstance(view.entry, dict) else {}
    items = entry.setdefault("entries", [])
    item = items[view.item] if view.item is not None and view.item < len(items) else None
    return entry, items, item


def _no_item() -> tuple[bool, Any]:
    from imgui_bundle import imgui

    imgui.text_disabled("Pick an item in the table (or + Add item) to edit it here.")
    return False, None


def draw_list_tab(ext, view: EntryView) -> tuple[bool, Any]:
    from imgui_bundle import imgui

    from creation_lib.ui.widgets.modern import semantic_color
    from ui.tools.catalog_editor import render
    from ui.tools.catalog_editor.features import leveled_lists
    from ui.tools.catalog_editor.model import ADDED, OVERRIDDEN

    entry = copy.deepcopy(view.entry)
    ctx = view.ctx
    props = _props(ctx.schema, view.doc.entry_schema(view.section))
    path = f"lists/{view.row.key}"
    changed = False
    imgui.begin_disabled(view.read_only)
    try:
        _heading("Which list this changes", "The leveled list that Tales rolls this way, and where it applies.")
        changed |= _field("List", props["target"], entry, "target", ctx, path, "The Fallout 4 leveled list to change.")
        changed |= _field("Converted from", props["source"], entry, "source", ctx, path,
                          "The Fallout 76 list this one was converted from. Leave as is unless you know you need to change it.")
        scopes = list(SCOPE_LABELS)
        index = scopes.index(entry.get("scope", "always")) if entry.get("scope", "always") in scopes else 1
        imgui.set_next_item_width(260)
        moved, index = imgui.combo(f"Applies##scope{path}", index, list(SCOPE_LABELS.values()))
        _help("'Only in the SeventySix world' means FO76 rules apply only to cells and locations from SeventySix.esm; "
              "Fallout 4's own areas keep the vanilla list. 'Everywhere' rolls it anywhere.")
        if moved:
            entry["scope"], changed = scopes[index], True
        changed |= _field("Quest", props["quest"], entry, "quest", ctx, path,
                          "Optional. A quest whose aliases the list's checks may look at.")
        _heading("How it rolls", "Tick the behaviours you want. Hover a box for what it does.")
        changed |= _draw_flags(entry)
        changed |= _field("Chance of nothing", props["chance_none"], entry, "chance_none", ctx, path,
                          "Percent chance that rolling the list gives nothing at all.")
        changed |= _field("Most of one item", props["max_count"], entry, "max_count", ctx, path,
                          "Upper limit on how many of one item a roll can give. 0 means no limit.")
        _heading("When the whole list applies", "Checks the list must pass before it is used. Without them it always applies.")
        picked, new = render.draw_value("List checks", props["conditions"], copy.deepcopy(entry.get("conditions") or []),
                                        ctx, f"{path}/conditions")
        if picked:
            entry["conditions"], changed = new, True
    finally:
        imgui.end_disabled()
    plugin_list = ext.plugin_list(view.forms, view.row.key)
    differences = leveled_lists.entry_differences(entry, plugin_list) if plugin_list is not None else []
    if differences:
        _heading("Compared with your plugin's record", "Where this catalog list no longer matches the LVLI/LVLN in the plugin.")
        imgui.text_colored(semantic_color("warning"), "The items differ from the plugin's:")
        for line in differences:
            imgui.bullet_text(line)
        if view.row.status in (ADDED, OVERRIDDEN) and imgui.small_button("Reset items from plugin"):
            entry["entries"] = leveled_lists.list_from_plugin(plugin_list)["entries"]
            changed = True
    return changed and not view.read_only, entry


def draw_item_tab(ext, view: EntryView) -> tuple[bool, Any]:
    from imgui_bundle import imgui

    from ui.tools.catalog_editor import render

    entry, items, item = _selected_item(view)
    if item is None:
        return _no_item()
    item_props = _props(view.ctx.schema, _props(view.ctx.schema, view.doc.entry_schema(view.section))["entries"]["items"])
    path = f"lists/{view.row.key}/{view.item}"
    changed = False
    picked, new = render.draw_value("Item", item_props["form"], item.get("form", ""), view.ctx, f"{path}/form")
    _help("The item, or another leveled list, this row gives.")
    if picked:
        item["form"], changed = new, True
    weighted = bool(int(entry.get("flags", 0)) & 0x200)
    titles = {"level": "From roll level", "count": "Count",
              "chance_none": "Weight" if weighted else "Chance of nothing %"}
    for key, title in titles.items():
        did, value = render.draw_value(title, item_props[key], copy.deepcopy(item.get(key) or {}), view.ctx, f"{path}/{key}")
        _help(_COLUMN_HELP[key])
        if did:
            item[key], changed = value, True
    imgui.text_disabled("Numbers can also follow a global value or a curve.")
    return changed, entry


def draw_checks_tab(ext, view: EntryView) -> tuple[bool, Any]:
    from imgui_bundle import imgui

    from ui.tools.catalog_editor import render

    entry, items, item = _selected_item(view)
    if item is None:
        return _no_item()
    imgui.text_wrapped("This item can be picked only when these checks pass. Otherwise the list picks from the other items.")
    item_props = _props(view.ctx.schema, _props(view.ctx.schema, view.doc.entry_schema(view.section))["entries"]["items"])
    picked, new = render.draw_value("", item_props["conditions"], copy.deepcopy(item.get("conditions") or []), view.ctx,
                                    f"lists/{view.row.key}/{view.item}/conditions")
    if picked:
        item["conditions"] = new
        return True, entry
    return False, entry


def draw_owner_tab(ext, view: EntryView) -> tuple[bool, Any]:
    entry, items, item = _selected_item(view)
    if item is None:
        return _no_item()
    return _draw_extra(item, view.ctx, f"lists/{view.row.key}/{view.item}"), entry


def draw_in_game_tab(ext, view: EntryView) -> tuple[bool, Any]:
    from imgui_bundle import imgui

    from creation_lib.ui.widgets.modern import semantic_color

    entry = view.entry if isinstance(view.entry, dict) else {}
    findings = ext.findings(view)
    if view.item is not None:
        items = entry.get("entries") or []
        if view.item < len(items):
            imgui.text_wrapped(item_sentence(items[view.item], view.name_of, bool(int(entry.get("flags", 0)) & 0x200)))
        findings = [finding for finding in findings if finding.item == view.item]
        preview, note = ext.preview(view)
        outcome = next((o for o in (preview.entries if preview else []) if o.index == view.item), None)
        imgui.separator_text("In this situation")
        if outcome is None:
            imgui.text_disabled(note or "No preview for this item.")
        else:
            imgui.text(f"Odds: {outcome.odds * 100:.1f}%" if outcome.odds is not None else "Can't drop here.")
            for reason in outcome.reasons:
                imgui.bullet_text(reason)
    else:
        imgui.text_wrapped(plain_summary(entry, view.name_of))
    imgui.separator_text("Will Tales roll it the FO76 way?")
    if not findings:
        imgui.text_colored(semantic_color("success"), "Yes: Tales can answer everything here.")
    for finding in findings:
        imgui.text_colored(semantic_color("error"), finding.text)
    return False, view.entry
