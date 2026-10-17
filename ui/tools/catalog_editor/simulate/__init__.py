"""Offline preview of how Tales rolls a LeveledLists catalog entry, for the editor's "what drops here" view.

The roll mirrors mods/B21_TalesFromAppalachia/src/LeveledListRoll.h; the shared vectors in
mods/B21_TalesFromAppalachia/tests/leveled-lists/roll_vectors.json keep the two in step.
"""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field


@dataclass(frozen=True)
class Situation:
    level: int = 25
    in_seventysix: bool = True
    region: str | None = None
    location_keywords: tuple[str, ...] = ()
    hour: int | None = None
    has_subject: bool = True
    subject_is_player: bool = False
    # Checks that can't be worked out offline, keyed by UnknownCheck.key; missing keys mean "ask".
    assume: dict[str, bool] = field(default_factory=dict)
    names: dict[str, str] = field(default_factory=dict)  # FormKey → display name, for plain-words reasons
    globals: dict[str, float] = field(default_factory=dict)  # FormKey → value, for value globals
    document: dict = field(default_factory=dict)  # merged curves / regions / condition_forms / delegates sections


@dataclass(frozen=True)
class UnknownCheck:
    key: str
    label: str
    entry_index: int | None  # None: a list-wide check


@dataclass
class EntryOutcome:
    index: int
    odds: float | None  # chance this entry is among the picks; None when it can't be picked
    eligible: bool
    reasons: list[str] = field(default_factory=list)  # plain-words why / why not
    unknowns: list[str] = field(default_factory=list)  # UnknownCheck keys this entry depends on


@dataclass
class ListPreview:
    used_by_tales: bool  # False: out of scope here, Fallout 4 rolls the plugin record
    falls_back: bool  # True: Tales hands the whole list to Fallout 4's roll
    fallback_reasons: list[str]
    chance_none: float
    entries: list[EntryOutcome]
    unknowns: list[UnknownCheck]
    notes: list[str] = field(default_factory=list)  # list-wide plain-words lines


ListResolver = Callable[[str], dict | None]  # FormKey → merged catalog entry of a child list, if any


def preview_list(entry: dict, situation: Situation, resolve_list: ListResolver | None = None) -> ListPreview:
    from ui.tools.catalog_editor.simulate import preview

    return preview.preview_list(entry, situation, resolve_list)


def sample(entry: dict, situation: Situation, rolls: int, seed: int = 0,
           resolve_list: ListResolver | None = None) -> dict[str, int]:
    """Forms picked, by number of rolls they appeared in; the key "" counts rolls that gave nothing.

    A list outside Tales' scope gives {}: Fallout 4 rolls it."""
    from ui.tools.catalog_editor.simulate import preview

    return preview.sample(entry, situation, rolls, seed, resolve_list)
