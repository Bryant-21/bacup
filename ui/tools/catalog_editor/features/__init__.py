"""Per-feature extensions of the catalog editor: section actions, validation, and the workbench hooks.

An extension is registered under the feature's name (the schema file's stem). Every hook is optional;
a feature without an extension gets the workbench's generic views (its entry's fields).
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Any

from ui.tools.catalog_editor.forms import FormSource
from ui.tools.catalog_editor.model import FeatureDocument, Problem, Row


@dataclass(frozen=True)
class SectionAction:
    label: str
    run: Callable[[], None]
    tooltip: str = ""


@dataclass(frozen=True)
class Badge:
    text: str
    role: str = "muted"  # a semantic_color role
    tooltip: str = ""


@dataclass(frozen=True)
class EntryProblem:
    """A finding shown in the banner above the selected entry's table."""

    message: str
    whole: bool = False  # True: it changes everything about the entry (a list handed to Fallout 4's roll)
    item: int | None = None  # index into the entry's main list, to link to
    check: int | None = None  # index into that item's checks
    blocking: bool = False


@dataclass(frozen=True)
class RemoveWording:
    """How "remove" reads for a feature whose removal means "Fallout 4 / Tales' default takes over"."""

    label: str
    tooltip: str
    removed_note: str


@dataclass
class EntryView:
    """Everything a hook needs about the selected entry, built once per frame by the workbench."""

    tool: Any
    doc: FeatureDocument
    section: str
    row: Row
    ctx: Any  # render.RenderContext
    read_only: bool
    master: str  # the master the inherited entry comes from, "" for an added entry
    item: int | None = None  # the selected item of the entry's main list
    select_item: Callable[[int | None], None] = lambda index: None
    focus_tab: Callable[[str], None] = lambda title: None
    names: dict[str, str] = field(default_factory=dict)

    @property
    def entry(self) -> Any:
        return self.row.entry

    @property
    def forms(self):
        return self.tool.forms

    def name_of(self, key: str) -> str:
        """A FormKey's EditorID when the load has it, else the key itself."""
        if not isinstance(key, str) or not key:
            return key or ""
        if key not in self.names:
            described = self.forms.describe(key) if hasattr(self.forms, "describe") else None
            self.names[key] = described.split(" [", 1)[0] if described else key
        return self.names[key]


Draw = Callable[[EntryView], tuple[bool, Any]]


@dataclass(frozen=True)
class ResultView:
    """The centre table's first tab: what the entry does. `draw` returns (changed, new_entry)."""

    title: str
    draw: Draw


@dataclass(frozen=True)
class InspectorTab:
    title: str
    draw: Draw


class FeatureExtension:
    def section_actions(self, tool, doc: FeatureDocument, section: str) -> list[SectionAction]:
        """Buttons drawn beside "New entry" on `section`'s toolbar."""
        return []

    def problems(self, doc: FeatureDocument, forms: FormSource | None) -> list[Problem]:
        """Extra findings for the validation panel; set `blocking=False` for warnings that must not stop a save."""
        return []

    def warm(self, forms: FormSource) -> None:
        """Slow reads of the loaded plugins, run off the UI thread while a plugin opens."""

    def draw_detail(self, tool, doc: FeatureDocument, section: str, row: Row) -> None:
        """imgui content for the inspector's "In game" tab."""

    def custom_form(self, doc: FeatureDocument, section: str) -> bool:
        """True when `draw_form` replaces the generic entry form for `section`."""
        return False

    def draw_form(self, tool, doc: FeatureDocument, section: str, row: Row, ctx, read_only: bool):
        """The entry form as (changed, new_entry), like render.draw_value; only called when `custom_form` is True."""
        return False, row.entry

    # -- workbench --

    def entry_name(self, tool, doc: FeatureDocument, section: str, row: Row) -> str | None:
        """The readable name the browser and header show; None uses the EditorID of the key or entry."""
        return None

    def browser_badges(self, tool, doc: FeatureDocument, section: str, row: Row) -> list[Badge]:
        """Short tags after the entry's name in the browser."""
        return []

    def status_badge(self, view: EntryView) -> Badge | None:
        """The header's verdict for the entry ("Tales rolls it the FO76 way"); None shows its layer status."""
        return None

    def remove_wording(self, doc: FeatureDocument, section: str) -> RemoveWording | None:
        return None

    def main_list(self, doc: FeatureDocument, section: str) -> tuple[str, str] | None:
        """(field, noun) of the list the layer strip counts ("entries", "items"); None picks the first list field."""
        return None

    def copy_notice(self, view: EntryView, new_entry: Any) -> str | None:
        """The confirmation shown before the author's first change to an inherited entry; None: the generic text."""
        return None

    def has_situation(self, doc: FeatureDocument, section: str) -> bool:
        return False

    def draw_situation(self, view: EntryView) -> None:
        """The situation bar above the result table; only called when `has_situation` is True."""

    def result_view(self, doc: FeatureDocument, section: str) -> ResultView | None:
        return None

    def inspector_tabs(self, view: EntryView) -> list[InspectorTab]:
        """Tabs that replace the inspector's generic "Fields" tab."""
        return []

    def sentence(self, view: EntryView) -> str:
        """Plain words on top of the inspector."""
        return ""

    def entry_problems(self, view: EntryView) -> list[EntryProblem] | None:
        """The banner's findings for the entry; None takes them from `problems` by key."""
        return None

    def describe_item(self, doc: FeatureDocument, section: str, field_name: str, item: Any,
                      name_of: Callable[[str], str]) -> str | None:
        """One line for an item of a list field in the diff views; None uses the generic text."""
        return None


REGISTRY: dict[str, FeatureExtension] = {}


def register(feature: str, extension: FeatureExtension) -> None:
    REGISTRY[feature] = extension


def extension_for(feature: str) -> FeatureExtension | None:
    from ui.tools.catalog_editor.features import leveled_lists  # noqa: F401  (registers itself)
    from ui.tools.catalog_editor.features import weapon_model_swaps, camp_emotes_reputation  # noqa: F401
    from ui.tools.catalog_editor.features import diseases, food_spoilage, gold_bullion, npc_stats, perk_cards  # noqa: F401

    return REGISTRY.get(feature)
