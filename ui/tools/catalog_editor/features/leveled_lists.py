"""LeveledLists extension: import a plugin's LVLI/LVLN as catalog rows, differences from the plugin, and Tales' fallback check.

The check mirrors Tales' ``LeveledListCatalog.h``, ``Fo76Conditions.h`` and ``LeveledListCompile.h``: a list the
check flags still saves, but Tales rolls it through Fallout 4 instead of the FO76 way.
"""

from __future__ import annotations

import struct
from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any

from creation_lib.catalog.plugin_catalog import parse_form_key
from creation_lib.esp.native_runtime import plugin_handle_group_record_summaries, plugin_handle_record_subrecords

from ui.tools.catalog_editor.features import (
    Badge, EntryProblem, EntryView, FeatureExtension, InspectorTab, RemoveWording, ResultView, SectionAction, register,
)
from ui.tools.catalog_editor.forms import PluginFormSource
from ui.tools.catalog_editor.model import ADDED, OVERRIDDEN, REMOVED, FeatureDocument, Problem
from ui.tools.catalog_editor.widgets import templates
from ui.tools.catalog_editor.widgets.functions import BY_ID, TALES_PROVIDERS

FEATURE = "LeveledLists"
LIST_SIGNATURES = ("LVLI", "LVLN")

FLAGS: tuple[tuple[int, str], ...] = (
    (0x01, "All levels"),
    (0x02, "Each in count"),
    (0x04, "Use all"),
    (0x08, "Disable if nothing"),
    (0x40, "First match"),
    (0x80, "Shift up"),
    (0x200, "Weighted"),
)
IGNORED_FLAGS: tuple[tuple[int, str], ...] = ((0x10, "Dispenser"), (0x20, "Don't show as marker"), (0x100, "Workshop list"))
KNOWN_FLAGS = sum(bit for bit, _ in FLAGS)
IGNORED_FLAG_MASK = sum(bit for bit, _ in IGNORED_FLAGS)

OPS = ("==", "!=", ">", ">=", "<", "<=")
RUN_ONS = ("subject", "target", "reference", "instance_owner", "active_players", "potential_players")
_KEYED_SECTIONS = ("condition_forms", "curves", "regions", "delegates")


def value(number: float = 0.0) -> dict:
    return {"value": number}


def _canon(key: str) -> str:
    """The key Tales looks a form up by: plugin case-insensitive, leading zeros immaterial."""
    parsed = parse_form_key(key) if isinstance(key, str) else None
    return f"{parsed[0]}:{parsed[1]:X}" if parsed else key


# --- Plugin side -----------------------------------------------------------------------------


@dataclass
class PluginEntry:
    form: str
    level: int
    count: int
    chance_none: int
    owner: str = ""
    owner_is_faction: bool = False
    owner_data: int = 0  # COED: faction rank, else a global's FormID resolved into owner_global
    owner_global: str = ""
    health: float | None = None


@dataclass
class PluginList:
    target: str
    editor_id: str
    signature: str
    flags: int
    chance_none: int
    max_count: int
    chance_global: str = ""
    entries: list[PluginEntry] = field(default_factory=list)


def _entry_json(entry: PluginEntry) -> dict:
    row = {"form": entry.form, "level": value(entry.level), "count": value(entry.count),
           "chance_none": value(entry.chance_none), "conditions": []}
    if entry.owner or entry.health is not None:
        extra: dict = {"health": 1.0 if entry.health is None else entry.health}
        if entry.owner:
            extra["owner"] = entry.owner
            if entry.owner_is_faction:
                extra["rank"] = entry.owner_data
            elif entry.owner_global:
                extra["global"] = entry.owner_global
        row["extra"] = extra
    return row


def list_from_plugin(plugin_list: PluginList) -> dict:
    """A catalog list that rolls exactly as the plugin authored it, ready for conditions."""
    chance = value(plugin_list.chance_none)
    if plugin_list.chance_global:
        chance["global"] = plugin_list.chance_global
    return {
        "source": plugin_list.target, "target": plugin_list.target, "editor_id": plugin_list.editor_id,
        "signature": plugin_list.signature, "scope": "always", "flags": plugin_list.flags & 0x07,
        "chance_none": chance, "max_count": value(plugin_list.max_count), "conditions": [],
        "entries": [_entry_json(entry) for entry in plugin_list.entries],
    }


def entry_differences(catalog_list: dict, plugin_list: PluginList) -> list[str]:
    """Where the catalog's entries no longer match the plugin's (form, level, count, by position)."""
    rows = catalog_list.get("entries", [])
    out = []
    if len(rows) != len(plugin_list.entries):
        out.append(f"{len(rows)} catalog entries, {len(plugin_list.entries)} in the plugin")
    for index, (row, entry) in enumerate(zip(rows, plugin_list.entries), start=1):
        parts = []
        if row.get("form") != entry.form:
            parts.append(f"form {row.get('form')} vs {entry.form}")
        if (row.get("level") or {}).get("value") != entry.level:
            parts.append(f"level {(row.get('level') or {}).get('value')} vs {entry.level}")
        if (row.get("count") or {}).get("value") != entry.count:
            parts.append(f"count {(row.get('count') or {}).get('value')} vs {entry.count}")
        if parts:
            out.append(f"entry {index}: " + ", ".join(parts))
    return out


def _read_list(forms: PluginFormSource, form_id: int, signature: str, editor_id: str) -> PluginList:
    row = PluginList(forms.key(form_id), editor_id, signature, 0, 0, 0)
    entry: PluginEntry | None = None
    for sig, data, _ in plugin_handle_record_subrecords(forms.plugin.handle, form_id) or []:
        if sig == "LVLD" and data:
            row.chance_none = data[0]
        elif sig == "LVLM" and data:
            row.max_count = data[0]
        elif sig == "LVLF" and data:
            row.flags = data[0]
        elif sig == "LVLG" and len(data) >= 4:
            global_id = struct.unpack_from("<I", data)[0]
            row.chance_global = forms.key(global_id) if global_id else ""
        elif sig == "LVLO" and len(data) >= 12:
            level, _, _, item, count, chance, _ = struct.unpack_from("<HBBIHBB", data)
            entry = PluginEntry(forms.key(item), level, count, chance)
            row.entries.append(entry)
        elif sig == "COED" and entry is not None and len(data) >= 12:
            owner, data_value, health = struct.unpack_from("<IIf", data)
            if owner:
                entry.owner = forms.key(owner)
                found = forms.resolve(entry.owner)
                entry.owner_is_faction = found is not None and found[0] == "FACT"
                if entry.owner_is_faction:
                    entry.owner_data = struct.unpack("<i", struct.pack("<I", data_value))[0]
                elif data_value:
                    entry.owner_global = forms.key(data_value)
            entry.health = health
    return row


def read_plugin_lists(forms: PluginFormSource) -> list[PluginList]:
    """The LVLI/LVLN records the open plugin defines or overrides, by EditorID."""
    lists = [
        _read_list(forms, summary.form_id, signature, summary.editor_id or "")
        for signature in LIST_SIGNATURES
        for summary in plugin_handle_group_record_summaries(forms.plugin.handle, signature)
    ]
    return sorted(lists, key=lambda row: row.editor_id.casefold())


# --- The check (Tales' FallbackOf) -------------------------------------------------------------

# Roles a key must fill, as Tales' FormRole. `Loaded(key, role)` answers whether the load has it.
Loaded = Callable[[str, str], bool]
_PARAM_ROLES = {579: "keyword", 8007: "keyword", 853: "recipe", 857: "quest"}

ROLE_SIGNATURES: dict[str, tuple[str, ...]] = {
    "list": ("LVLI", "LVLN"),
    "global": ("GLOB",),
    "reference": ("REFR", "ACHR", "PGRE", "PMIS", "PARW", "PBAR", "PBEA", "PCON", "PFLA", "PHZD"),
    "keyword": ("KYWD",),
    "recipe": ("COBJ",),
    "quest": ("QUST",),
    "owner": ("NPC_", "FACT"),
}


def role_accepts(role: str, signature: str) -> bool:
    """Entry, value-global and any-role keys only need to exist (Tales' FormRole)."""
    accepted = ROLE_SIGNATURES.get(role)
    return accepted is None or signature in accepted


def forms_loaded(forms, key: str, role: str) -> bool:
    found = forms.resolve(key)
    return found is not None and role_accepts(role, found[0])


def _is_form(param) -> bool:
    return isinstance(param, dict) and "raw" not in param


def _kind(condition: dict) -> str:
    """'native', 'provider', 'fixed' or 'unsupported', as Tales' RuleFor."""
    function = condition.get("function")
    params = condition.get("params") or [None, None]
    second = params[1] if len(params) > 1 else None
    if function == 149 and _is_form(second):
        return "provider"
    if params[0] is None and function in (72, 359, 853, 74):
        return "fixed"
    known = BY_ID.get(function)
    if known is None:
        return "unsupported"
    if function in TALES_PROVIDERS:
        return "provider"
    return "fixed" if known.note.startswith("Always") else "native"


def _provider_resolved(condition: dict, document: dict) -> bool:
    params = condition.get("params") or [None, None]
    param = params[0]
    form = _is_form(param)
    function = condition["function"]
    regions = document.get("regions", {})
    if function == 938:
        if form:
            return parse_form_key(param.get("source", "")) is not None
        return bool(param) and 0 < int(param.get("raw", 0)) < 0x1000000
    if function == 846:
        if form:
            key = parse_form_key(param.get("source", ""))
            return key is not None and key[1] == 0x129D
        return bool(param) and param.get("raw") == 0x129D
    if function == 149:
        region = regions.get(_canon((params[1] or {}).get("source", "")))
        return form and bool(param.get("form")) and bool(region) and bool(region.get("world")) and bool(region.get("polygons"))
    if function in (579, 8007, 853, 857):
        return form and bool(param.get("form"))
    if function in (737, 906):
        return form and parse_form_key(param.get("source", "")) is not None
    if function == 849:
        region = regions.get(_canon(param.get("source", ""))) if form else None
        if not region:
            return False
        return bool(param.get("form")) if region.get("nuke") else bool(region.get("world")) and bool(region.get("polygons"))
    if function == 875:
        return (form and _canon(param.get("source", "")) in document.get("condition_forms", {})
                and condition.get("run_on") in ("subject", "target") and not condition.get("global"))
    return True


@dataclass(frozen=True)
class Fallback:
    """One reason Tales hands a list to Fallout 4's roll, and the item and check it comes from."""

    reason: str
    where: str = ""
    item: int | None = None  # index into the list's entries; None: the list itself
    check: int | None = None  # index into that item's (or the list's) checks

    @property
    def text(self) -> str:
        return f"{self.where}: {self.reason}" if self.where else self.reason


NameOf = Callable[[str], str]


def item_where(index: int, entry: dict, name_of: NameOf) -> str:
    form = entry.get("form", "")
    name = name_of(form) if form else ""
    return f"item {index + 1} ({name or 'no item picked'})"


def check_where(index: int, condition: dict, name_of: NameOf) -> str:
    """'check 1 "The player has perk X" (HasPerk, 448)', without the sentence when no builder check fits."""
    function = condition.get("function")
    known = BY_ID.get(function)
    label = f"{known.name}, {function}" if known else f"function {function}"
    if templates.match(condition) is None:
        return f"check {index + 1} ({label})"
    return f'check {index + 1} "{templates.sentence(condition, name_of)}" ({label})'


def _value_problems(row: dict | None, document: dict, where: str, item: int | None) -> list[Fallback]:
    row = row or {}
    out = []
    if row.get("unresolved"):
        out.append(Fallback("unresolved value", where, item))
    if row.get("curve") and _canon(row["curve"]) not in document.get("curves", {}):
        out.append(Fallback(f"curve {row['curve']} is not in the file", where, item))
    return out


def _walk_conditions(rows: list, document: dict, quest: str, where: str, out: list[Fallback], visiting: set[str],
                     visit: Callable[[dict], None], item: int | None, name_of: NameOf, top: int | None = None) -> None:
    for index, condition in enumerate(rows):
        visit(condition)
        check = index if top is None else top
        function = condition.get("function")
        name = f"{where}, {check_where(index, condition, name_of)}"

        def fail(reason: str) -> None:
            out.append(Fallback(reason, name, item, check))

        if condition.get("op") not in OPS:
            fail(f"operator {condition.get('op')!r} is not one of {', '.join(OPS)}")
            continue
        if condition.get("unresolved") or condition.get("unsupported_flags"):
            fail("unresolved")
            continue
        run_on = condition.get("run_on")
        if run_on not in RUN_ONS:
            fail(f"run on {run_on!r} is not supported")
            continue
        if run_on in ("active_players", "potential_players") and quest:
            fail("a player-set run-on with a list quest is not supported")
            continue
        kind = _kind(condition)
        if kind == "unsupported":
            fail("Tales doesn't answer this function")
            continue
        if kind == "provider" and not _provider_resolved(condition, document):
            fail("its parameters don't fit what Tales needs")
            continue
        if function != 875:
            continue
        source = condition["params"][0]["source"]
        if _canon(source) in visiting:
            fail(f"condition form {source} refers back to itself")
            continue
        visiting.add(_canon(source))
        _walk_conditions(document["condition_forms"][_canon(source)], document, quest, f"{name}, condition form {source}",
                         out, visiting, visit, item, name_of, check)
        visiting.discard(_canon(source))


def _canonical(document: dict) -> dict:
    out = dict(document)
    for section in _KEYED_SECTIONS:
        out[section] = {_canon(key): item for key, item in (document.get(section) or {}).items()}
    return out


_LIST_VALUES = {"chance_none": "the list's chance of nothing", "max_count": "the list's most of one item"}
_ITEM_VALUES = {"level": "level", "count": "count", "chance_none": "chance of nothing"}


def fallback_findings(catalog_list: dict, document: dict, loaded: Loaded | None = None,
                      name_of: NameOf = lambda key: key) -> list[Fallback]:
    """Why Tales would roll this list through Fallout 4 instead; empty means it rolls FO76's way."""
    document = _canonical(document)
    out: list[Fallback] = []
    required: list[tuple[str, str, int | None]] = [(catalog_list.get("target", ""), "list", None)]
    current: list[int | None] = [None]

    def require_value(row: dict | None) -> None:
        if row and row.get("global"):
            required.append((row["global"], "value_global", current[0]))

    def visit(condition: dict) -> None:
        if condition.get("global"):
            required.append((condition["global"], "global", current[0]))
        if condition.get("reference"):
            required.append((condition["reference"], "reference", current[0]))
        for param in condition.get("params") or []:
            if _is_form(param) and param.get("form"):
                required.append((param["form"], _PARAM_ROLES.get(condition.get("function"), "any"), current[0]))

    for key, label in _LIST_VALUES.items():
        out += _value_problems(catalog_list.get(key), document, label, None)
        require_value(catalog_list.get(key))
    flags = int(catalog_list.get("flags", 0))
    if flags & ~(KNOWN_FLAGS | IGNORED_FLAG_MASK):
        out.append(Fallback(f"unknown flags {flags & ~(KNOWN_FLAGS | IGNORED_FLAG_MASK):#x}"))
    quest = catalog_list.get("quest", "")
    _walk_conditions(catalog_list.get("conditions", []), document, quest, "the list", out, set(), visit, None, name_of)
    entries = catalog_list.get("entries", [])
    for index, entry in enumerate(entries):
        current[0] = index
        where = item_where(index, entry, name_of)
        required.append((entry.get("form", ""), "entry", index))
        delegate = document["delegates"].get(_canon(entry.get("form", "")))
        if delegate:
            required.append((delegate, "list", index))
        for key, label in _ITEM_VALUES.items():
            out += _value_problems(entry.get(key), document, f"{where}, {label}", index)
            require_value(entry.get(key))
        extra = entry.get("extra") or {}
        if extra.get("owner"):
            required.append((extra["owner"], "owner", index))
        if extra.get("global"):
            required.append((extra["global"], "global", index))
        _walk_conditions(entry.get("conditions", []), document, quest, where, out, set(), visit, index, name_of)
    if loaded is not None:
        seen = set()
        for key, role, index in required:
            if key and key not in seen and not loaded(key, role):
                seen.add(key)
                where = item_where(index, entries[index], name_of) if index is not None else ""
                out.append(Fallback(f"form {key} not loaded or not a {role.replace('_', ' ')}", where, index))
    return out


def fallback_reasons(catalog_list: dict, document: dict, loaded: Loaded | None = None,
                     name_of: NameOf = lambda key: key) -> list[str]:
    return [finding.text for finding in fallback_findings(catalog_list, document, loaded, name_of)]


def roll_summary(row: dict) -> str:
    flags = int(row.get("flags", 0))
    count = len(row.get("entries", []))
    parts = [f"Gives every item that passes, of {count}" if flags & 0x04 else f"Picks one of {count} items"]
    chance = (row.get("chance_none") or {}).get("value", 0)
    if chance or (row.get("chance_none") or {}).get("global"):
        parts.append(f"{chance:g}% chance of nothing" + (" (from a global)" if row["chance_none"].get("global") else ""))
    if flags & 0x02:
        parts.append("each count rolls separately")
    if flags & 0x40:
        parts.append("first passing item wins")
    if flags & 0x200:
        parts.append("weighted")
    checked = sum(bool(entry.get("conditions")) for entry in row.get("entries", []))
    if checked:
        parts.append(f"{checked} item(s) have checks")
    if row.get("conditions"):
        parts.append("the list has checks")
    return ". ".join(part[0].upper() + part[1:] for part in parts) + "."


def flag_names(flags: int) -> str:
    names = [name for bit, name in (*FLAGS, *IGNORED_FLAGS) if flags & bit]
    unknown = flags & ~(KNOWN_FLAGS | IGNORED_FLAG_MASK)
    if unknown:
        names.append(f"unknown {unknown:#x}")
    return ", ".join(names) or "none"


# --- The merged document as Tales sees it ------------------------------------------------------


def merged_view(doc: FeatureDocument) -> dict:
    """The keyed sections after every master's catalog and this plugin's own layer merged."""
    return {section: {row.key: row.entry for row in doc.rows(section) if row.status != REMOVED}
            for section in _KEYED_SECTIONS if section in doc.sections()}


def _loader(forms) -> Loaded | None:
    return (lambda key, role: forms_loaded(forms, key, role)) if hasattr(forms, "resolve") else None


class _ListSource:
    """The picker's candidate list: the open plugin's own leveled lists."""

    def __init__(self, lists: list[PluginList], in_catalog: set[str]):
        self.lists = lists
        self.in_catalog = in_catalog

    def search(self, text: str, signatures=None, limit: int = 200) -> list[tuple[str, str]]:
        needle = text.casefold()
        out = []
        for row in self.lists:
            if needle and needle not in row.editor_id.casefold() and needle not in row.target.casefold():
                continue
            marker = "* " if _canon(row.target) in self.in_catalog else ""
            out.append((row.target, f"{marker}{row.editor_id or '<no EditorID>'} [{row.signature}] {row.target}  "
                                    f"{len(row.entries)} items"))
        return out[:limit]


class LeveledListsExtension(FeatureExtension):
    def __init__(self) -> None:
        from ui.tools.catalog_editor.features.leveled_form import PreviewCache, SituationState

        self._cache: tuple[object, list[PluginList], dict[str, PluginList]] | None = None
        self._merged: tuple[Any, dict, dict] | None = None
        self._findings: dict[tuple, list[Fallback]] = {}
        self._preview = PreviewCache()
        self.situation = SituationState()
        self.only_eligible = False

    def _plugin_lists(self, forms) -> list[PluginList]:
        if not isinstance(forms, PluginFormSource):
            return []
        if self._cache is None or self._cache[0] is not forms:
            lists = read_plugin_lists(forms)
            by_key: dict[str, PluginList] = {}
            for row in lists:
                by_key.setdefault(_canon(row.target), row)
            self._cache = (forms, lists, by_key)
        return self._cache[1]

    def warm(self, forms) -> None:
        self._plugin_lists(forms)

    def plugin_list(self, forms, key: str) -> PluginList | None:
        if not self._plugin_lists(forms):
            return None
        return self._cache[2].get(_canon(key))

    def section_actions(self, tool, doc, section):
        if section != "lists" or not isinstance(tool.forms, PluginFormSource):
            return []
        return [SectionAction("Import from plugin...", lambda: self._import(tool, doc),
                              "Copy one of the open plugin's leveled lists in, rolling exactly as it does now.")]

    def _import(self, tool, doc: FeatureDocument) -> None:
        forms = tool.forms
        lists = self._plugin_lists(forms)

        def catalog_keys() -> dict[str, str]:
            return {_canon(row.key): row.key for row in doc.rows("lists") if row.status != REMOVED}

        def apply(key: str) -> None:
            if not key:
                return
            plugin_list = self.plugin_list(forms, key)
            if plugin_list is None:
                tool.notify(f"{key} is not a list {forms.name} defines or overrides.", error=True)
                return
            present = catalog_keys()
            if _canon(key) in present:
                tool.select_row(present[_canon(key)])
                tool.notify(f"{plugin_list.editor_id or key} is already in the catalog.")
                return
            tool.select_row(doc.set_entry("lists", list_from_plugin(plugin_list)))

        tool.pick_record(LIST_SIGNATURES, apply, source=_ListSource(lists, set(catalog_keys())))

    def problems(self, doc, forms):
        if doc.name != FEATURE:
            return []
        view, loaded = merged_view(doc), _loader(forms) if forms is not None else None
        return [
            Problem(doc.name, "lists", row.key, f"Tales would roll this list through Fallout 4: {reason}", blocking=False)
            for row in doc.rows("lists") if row.status in (ADDED, OVERRIDDEN) and isinstance(row.entry, dict)
            for reason in fallback_reasons(row.entry, view, loaded, _namer(forms))
        ]

    # -- the merged document and the roll --

    def merged(self, doc: FeatureDocument) -> tuple[dict, dict]:
        """(merged keyed sections, merged lists by canonical target), cached per edit."""
        key = (id(doc), getattr(doc, "revision", None))
        if self._merged is None or self._merged[0] != key:
            lists = {_canon(row.key): row.entry for row in doc.rows("lists")
                     if row.status != REMOVED and isinstance(row.entry, dict)} if "lists" in doc.sections() else {}
            self._merged = (key, merged_view(doc), lists)
        return self._merged[1], self._merged[2]

    def findings(self, view: EntryView) -> list[Fallback]:
        if view.row.status == REMOVED or not isinstance(view.entry, dict):
            return []
        key = (id(view.doc), getattr(view.doc, "revision", None), view.row.key)
        if key not in self._findings:
            if len(self._findings) > 256:
                self._findings.clear()
            document, _ = self.merged(view.doc)
            loaded = _loader(view.forms) if view.forms is not None else None
            self._findings[key] = fallback_findings(view.entry, document, loaded, view.name_of)
        return self._findings[key]

    def preview(self, view: EntryView):
        from ui.tools.catalog_editor.features.leveled_form import run_preview

        entry = view.entry if isinstance(view.entry, dict) else {}
        key = (id(view.doc), getattr(view.doc, "revision", None), view.row.key, self.situation.signature())
        cache = self._preview
        if cache.key != key:
            document, lists = self.merged(view.doc)
            cache.preview, cache.note = run_preview(entry, self.situation.situation(view, entry, document), lists.get)
            cache.unknown_keys = [unknown.key for unknown in cache.preview.unknowns] if cache.preview is not None else []
            if cache.unknown_keys and self.situation.assume:
                situation = self.situation.situation(view, entry, document, cache.unknown_keys)
                cache.preview, cache.note = run_preview(entry, situation, lists.get)
            cache.key = key
            self.situation.unknown_count = len(cache.unknown_keys)
        return cache.preview, cache.note

    def histogram_key(self, view: EntryView) -> tuple:
        return view.row.key, getattr(view.doc, "revision", None), self.situation.signature()

    def roll(self, view: EntryView) -> None:
        from ui.tools.catalog_editor.features.leveled_form import run_sample

        entry = view.entry if isinstance(view.entry, dict) else {}
        document, lists = self.merged(view.doc)
        counts, note = run_sample(entry, self.situation.situation(view, entry, document, self._preview.unknown_keys), lists.get)
        if counts is None:
            view.tool.notify(f"Can't roll: {note}.", error=True)
            self.situation.histogram = None
        else:
            self.situation.histogram = (self.histogram_key(view), counts)

    def restore_warning(self, view: EntryView, master_entries: list, entries: list, master_index: int) -> str:
        """Warns, before it happens, when restoring a removed item would hand the whole list to Fallout 4."""
        document, _ = self.merged(view.doc)
        restored = {**view.entry, "entries": [*entries, master_entries[master_index]]}
        before = len(fallback_findings(view.entry, document, None, view.name_of))
        after = fallback_findings(restored, document, None, view.name_of)
        if len(after) <= before:
            return ""
        return ("Restoring this item would hand the whole list to Fallout 4's roll: " + after[-1].reason +
                f" ({after[-1].where.split(', ', 1)[-1]}).")

    @staticmethod
    def roll_line(entry: dict) -> str:
        return roll_summary(entry) + (" Only in the SeventySix world." if entry.get("scope") == "seventysix" else "")

    # -- workbench hooks --

    def browser_badges(self, tool, doc, section, row):
        if section == "lists" and row.status == REMOVED:
            return [Badge("FO4 roll", "muted", "Sent to Fallout 4's roll")]
        return []

    def status_badge(self, view):
        if view.section != "lists":
            return None
        if view.row.status == REMOVED:
            return Badge("Fallout 4 rolls it", "muted", "Your plugin sent this list to Fallout 4's roll.")
        if self.findings(view):
            return Badge("Falls back to Fallout 4's roll", "error", "Tales can't answer everything here; see the banner.")
        where = " (SeventySix world only)" if isinstance(view.entry, dict) and view.entry.get("scope") == "seventysix" else ""
        return Badge(f"Tales rolls it the FO76 way{where}", "success")

    def remove_wording(self, doc, section):
        if section != "lists":
            return None
        return RemoveWording(
            "Send list to Fallout 4's roll",
            "Doesn't delete any loot. Your plugin removes this list from the catalog, so Tales stops rolling it and "
            "Fallout 4 rolls the plugin's own record as normal.",
            "Sent to Fallout 4's roll: Tales doesn't roll this list, Fallout 4 rolls the plugin's own record.")

    def main_list(self, doc, section):
        return ("entries", "items") if section == "lists" else None

    def copy_notice(self, view, new_entry):
        if view.section != "lists" or not isinstance(view.entry, dict) or not isinstance(new_entry, dict):
            return None
        from ui.tools.catalog_editor.workbench import REMOVE, diff_entry

        master = view.master or "its master"
        count = len(view.entry.get("entries") or [])
        left_out = [view.name_of(line.before.get("form", "")) for line in diff_entry(view.entry, new_entry)
                    if line.kind == REMOVE and line.field == "entries" and isinstance(line.before, dict)]
        detail = f" with {', '.join(left_out)} left out" if left_out else ""
        return (f"This list belongs to {master}. To change it, your plugin saves its own copy of the whole list "
                f"({count} items){detail}. From now on, changes {master} makes to this list won't reach you.\n\n"
                "Wanted Fallout 4 to roll this list normally instead? Use \"Send list to Fallout 4's roll\" on the list.")

    def has_situation(self, doc, section):
        return section == "lists"

    def draw_situation(self, view):
        from ui.tools.catalog_editor.features.leveled_form import draw_situation

        draw_situation(self, view)

    def result_view(self, doc, section):
        if section != "lists":
            return None
        from ui.tools.catalog_editor.features.leveled_form import draw_drops

        return ResultView("What drops", lambda view: draw_drops(self, view))

    def inspector_tabs(self, view):
        if view.section != "lists" or not isinstance(view.entry, dict):
            return []
        from ui.tools.catalog_editor.features import leveled_form as form

        items = view.entry.get("entries") or []
        checks = len(items[view.item].get("conditions") or []) if view.item is not None and view.item < len(items) else 0
        return [
            InspectorTab("List", lambda v: form.draw_list_tab(self, v)),
            InspectorTab("Item & numbers", lambda v: form.draw_item_tab(self, v)),
            InspectorTab(f"Checks ({checks})", lambda v: form.draw_checks_tab(self, v)),
            InspectorTab("Owner & condition", lambda v: form.draw_owner_tab(self, v)),
            InspectorTab("In game", lambda v: form.draw_in_game_tab(self, v)),
        ]

    def sentence(self, view):
        if view.section != "lists" or not isinstance(view.entry, dict):
            return ""
        from ui.tools.catalog_editor.features.leveled_form import item_sentence, plain_summary

        items = view.entry.get("entries") or []
        if view.item is not None and view.item < len(items):
            return item_sentence(items[view.item], view.name_of, bool(int(view.entry.get("flags", 0)) & 0x200))
        return plain_summary(view.entry, view.name_of)

    def entry_problems(self, view):
        if view.section != "lists":
            return None
        return [EntryProblem(f"Tales hands this whole list to Fallout 4's roll: {finding.text}", whole=True,
                             item=finding.item, check=finding.check) for finding in self.findings(view)]

    def describe_item(self, doc, section, field_name, item, name_of):
        if section != "lists" or field_name != "entries" or not isinstance(item, dict):
            return None
        checks = len(item.get("conditions") or [])
        text = (f"{name_of(item.get('form', '')) or '(no item)'} · lvl {(item.get('level') or {}).get('value', 0):g}"
                f" · x{(item.get('count') or {}).get('value', 0):g}")
        chance = (item.get("chance_none") or {}).get("value", 0)
        return text + (f" · none {chance:g}%" if chance else "") + (f" · {checks} check{'s' * (checks != 1)}" if checks else "")


def _namer(forms) -> NameOf:
    def name_of(key: str) -> str:
        described = forms.describe(key) if hasattr(forms, "describe") and key else None
        return described.split(" [", 1)[0] if described else key

    return name_of


register(FEATURE, LeveledListsExtension())
