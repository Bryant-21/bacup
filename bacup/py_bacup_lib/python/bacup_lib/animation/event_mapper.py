"""Translates animation event names between game formats.

FO3/FNV use NiTextKeyExtraData text keys in .kf files.
FO4 uses hkaAnnotationTrack annotations in .hkx files.
This mapper handles the bidirectional translation between these formats.
"""
from __future__ import annotations

import re
from typing import TYPE_CHECKING

import yaml

from bacup_lib.models import AnimationEvent
from bacup_lib.native_maps import native_translation_maps_dir

if TYPE_CHECKING:
    pass

# Game name normalization for file lookup
_GAME_ALIASES: dict[str, str] = {
    "fo3": "fo3",
    "fallout3": "fo3",
    "fnv": "fo3",       # FNV shares FO3 animation format
    "falloutnv": "fo3",
    "fo4": "fo4",
    "fallout4": "fo4",
    "fo76": "fo76",
    "fallout76": "fo76",
}


class EventMapper:
    def __init__(self, source_game: str, target_game: str) -> None:
        """Load event mapping table for this game pair.

        Args:
            source_game: Source game identifier (e.g. "fo3", "fnv", "fo4").
            target_game: Target game identifier.

        Raises:
            FileNotFoundError: If no mapping YAML exists for this game pair.
        """
        src = _GAME_ALIASES.get(source_game.lower(), source_game.lower())
        tgt = _GAME_ALIASES.get(target_game.lower(), target_game.lower())

        map_file = native_translation_maps_dir() / f"events_{src}_to_{tgt}.yaml"
        if not map_file.exists():
            raise FileNotFoundError(
                f"No event mapping found: {map_file.name}"
            )

        with open(map_file, "r", encoding="utf-8") as f:
            data = yaml.safe_load(f)

        self._drop: set[str] = set(data.get("drop") or [])
        self._events: dict[str, str] = dict(data.get("events") or {})

        # Pre-compile regex patterns
        self._patterns: list[tuple[re.Pattern, str | None]] = []
        for entry in data.get("patterns") or []:
            pattern = re.compile(entry["match"])
            target = entry.get("target")  # None means drop
            self._patterns.append((pattern, target))

    def map_event(
        self, event: AnimationEvent
    ) -> tuple[AnimationEvent | None, str | None]:
        """Map a single animation event.

        Returns:
            A tuple of (mapped_event, warning).
            - mapped_event is None when the event is intentionally dropped.
            - warning is a string for unmapped events passed through verbatim,
              or None when the mapping was found (including drops).
        """
        text = event.text

        # Check drop list
        if text in self._drop:
            return None, None

        # Check direct mapping
        if text in self._events:
            mapped_text = self._events[text]
            return AnimationEvent(time=event.time, text=mapped_text), None

        # Check regex patterns
        for pattern, target in self._patterns:
            m = pattern.match(text)
            if m:
                if target is None:
                    # Pattern matched but target is null → drop
                    return None, None
                # Replace {N} placeholders with capture groups
                mapped_text = target
                for i, group in enumerate(m.groups(), start=1):
                    mapped_text = mapped_text.replace(f"{{{i}}}", group or "")
                return AnimationEvent(time=event.time, text=mapped_text), None

        # Unmapped — pass through verbatim with warning
        warning = f"Unmapped animation event '{text}' passed through as-is"
        return event, warning

    def map_events(
        self, events: tuple[AnimationEvent, ...]
    ) -> tuple[tuple[AnimationEvent, ...], list[str]]:
        """Map all events in a sequence.

        Returns:
            A tuple of (mapped_events, warnings).
            Dropped events are excluded from the output tuple.
        """
        mapped: list[AnimationEvent] = []
        warnings: list[str] = []

        for event in events:
            result, warning = self.map_event(event)
            if result is not None:
                mapped.append(result)
            if warning is not None:
                warnings.append(warning)

        return tuple(mapped), warnings
