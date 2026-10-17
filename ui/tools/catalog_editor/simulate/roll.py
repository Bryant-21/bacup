"""Port of Tales' ``LeveledListRoll.h``: FO76's CalculateCurrentFormList over a catalog list (JSON dicts)."""

from __future__ import annotations

from collections.abc import Callable, Sequence
from dataclasses import dataclass, field
from typing import Protocol

ALL_LEVELS = 0x01
EACH_IN_COUNT = 0x02
USE_ALL = 0x04
DISABLE_IF_NOTHING = 0x08
FIRST_MATCH = 0x40
SHIFT_UP = 0x80
WEIGHTED = 0x200

FORCE_NEVER, FORCE_DEFAULT, FORCE_ALWAYS, FORCE_SHIFT_UP = -1, 0, 1, 2
FORCES = {"never": FORCE_NEVER, "default": FORCE_DEFAULT, "always": FORCE_ALWAYS, "shift_up": FORCE_SHIFT_UP}


class Random(Protocol):
    def integer(self, minimum: int, maximum_exclusive: int) -> int: ...
    def unit(self) -> float: ...


class Host(Protocol):
    def has_subject(self) -> bool: ...
    def passes(self, conditions: Sequence[dict]) -> bool: ...
    def global_value(self, key: str) -> float | None: ...
    def list_passed(self, level: int) -> int: ...


@dataclass
class Pick:
    entry: dict
    form: str
    quantity: int = 1


@dataclass
class RollResult:
    picks: list[Pick] = field(default_factory=list)
    multiplier: int = 1
    max_count: int = 0


def curve_at(points: Sequence[Sequence[float]], x: float) -> float:
    if not points:
        return 0.0
    points = sorted(points, key=lambda point: point[0])
    if x <= points[0][0]:
        return points[0][1]
    if x >= points[-1][0]:
        return points[-1][1]
    upper = next(i for i, point in enumerate(points) if x < point[0])
    (x0, y0), (x1, y1) = points[upper - 1], points[upper]
    return y1 if x1 == x0 else y0 + (y1 - y0) * (x - x0) / (x1 - x0)


def byte(value: float) -> int:
    if not value > 0:
        return 0
    return int(min(value, 16777215.0)) & 255


def chance(value: float) -> int:
    return min(100, byte(value))


def quantity(value: float) -> int:
    if not value >= 1:
        return 1
    return int(min(value, 65535.0))


def emit_use_all(picks: Sequence[Pick], max_count: int, emit: Callable[[Pick], int]) -> int:
    total = 0
    for pick in picks:
        total = emit(pick)
        if max_count and total >= max_count:
            return max_count
    return total


def disables_subject_when_empty(row: dict, depth: int, template_roll: bool, subject_is_player: bool) -> bool:
    return bool(int(row.get("flags", 0)) & DISABLE_IF_NOTHING) and row.get("signature") == "LVLN" and depth == 1 \
        and template_roll and not subject_is_player


class Roller:
    def __init__(self, curves: dict, delegates: dict, host: Host, random: Random):
        self.curves = curves
        self.delegates = delegates
        self.host = host
        self.random = random

    def roll(self, row: dict, level: int, count: int, force: int = FORCE_DEFAULT) -> RollResult:
        result = RollResult()
        if count == 0:
            return result
        level = max(level, 1)
        flags = int(row.get("flags", 0))
        if self.host.has_subject() and not self.host.passes(row.get("conditions") or []):
            return result
        if flags & EACH_IN_COUNT:
            for _ in range(count):
                entry = self._single(row, level, force)
                if entry is not None:
                    result.picks.append(self._emit(entry))
        elif flags & USE_ALL:
            if self._list_chance_none(row):
                return result
            for entry in self._use_all(row, self.host.list_passed(level)):
                result.picks.append(self._emit(entry))
            result.multiplier = count
            result.max_count = byte(self.resolve(row.get("max_count")))
        else:
            entry = self._single(row, level, force)
            if entry is not None:
                result.picks.append(self._emit(entry))
            result.multiplier = count
        return result

    def resolve(self, value: dict | None, default: float = 0.0) -> float:
        value = value or {}
        literal = float(value.get("value", default))
        source = literal
        if value.get("global"):
            found = self.host.global_value(value["global"])
            if found is not None:
                source = found
        curve = value.get("curve")
        if not curve:
            return source
        points = self.curves.get(curve)
        if points is None:
            return literal
        return curve_at(points, source)

    def entry_order(self, row: dict) -> list[dict]:
        order: list[dict] = []
        levels: list[int] = []
        record_order = bool(int(row.get("flags", 0)) & (FIRST_MATCH | WEIGHTED))
        for entry in row.get("entries") or []:
            level = self.level_of(entry)
            at = len(order)
            if not record_order and order:
                high, low, found = len(order) - 1, 0, None
                while found is None and low <= high:
                    mid = low + (high - low) // 2
                    if level <= levels[mid]:
                        found = mid
                    else:
                        low = mid + 1
                at = low if found is None else found
            order.insert(at, entry)
            levels.insert(at, level)
        return order

    def level_of(self, entry: dict) -> int:
        value = self.resolve(entry.get("level"), 1.0)
        return int(min(value, 65535.0)) if value > 0 else 0

    def _list_chance_none(self, row: dict) -> bool:
        percent = chance(self.resolve(row.get("chance_none")))
        return percent != 0 and self.random.integer(0, 100) < percent

    def _entry_chance_none(self, entry: dict) -> int:
        return chance(self.resolve(entry.get("chance_none")))

    def _single(self, row: dict, level: int, force: int) -> dict | None:
        if self._list_chance_none(row):
            return None
        candidates = self.candidates(row, self.host.list_passed(level), force)
        if not candidates:
            return None
        return self._weighted(candidates) if int(row.get("flags", 0)) & WEIGHTED else self._uniform(candidates)

    def candidates(self, row: dict, roll_level: int, force: int) -> list[dict]:
        flags = int(row.get("flags", 0))
        all_levels = force == FORCE_ALWAYS or (force == FORCE_DEFAULT and bool(flags & ALL_LEVELS))
        shift = force == FORCE_SHIFT_UP and bool(flags & SHIFT_UP)
        level = roll_level
        baseline = -1 if all_levels else level
        band = 0
        reached = False
        out: list[dict] = []
        for entry in self.entry_order(row):
            entry_level = self.level_of(entry)
            if entry_level > level:
                if not shift:
                    break
                out.clear()
                band = 0
                baseline = level = entry_level
                shift = False
            if entry_level > band and (band == 0 or not reached):
                if entry_level >= baseline:
                    reached = True
                band = entry_level
                if not all_levels:
                    out.clear()
            if self.host.passes(entry.get("conditions") or []):
                out.append(entry)
            if flags & FIRST_MATCH and out:
                break
        return out

    def _uniform(self, candidates: list[dict]) -> dict | None:
        entry = candidates[self.random.integer(0, len(candidates))]
        percent = self._entry_chance_none(entry)
        if percent and self.random.integer(0, 100) <= percent:
            return None
        return entry

    def weight(self, entry: dict) -> int:
        value = self.resolve(entry.get("chance_none"))
        return int(min(value, 65535.0)) if value > 0 else 0

    def _weighted(self, candidates: list[dict]) -> dict | None:
        total = sum(self.weight(entry) for entry in candidates)
        roll = int(self.random.unit() * total)
        cumulative = 0
        for entry in candidates:
            cumulative += self.weight(entry)
            if roll <= cumulative:
                return entry
        return None

    def _use_all(self, row: dict, level: int) -> list[dict]:
        out = []
        for entry in row.get("entries") or []:
            if self.level_of(entry) > level:
                continue
            percent = self._entry_chance_none(entry)
            if percent and self.random.integer(0, 100) <= percent:
                continue
            if not self.host.passes(entry.get("conditions") or []):
                continue
            out.append(entry)
        return out

    def _emit(self, entry: dict) -> Pick:
        form = entry.get("form", "")
        return Pick(entry, self.delegates.get(form, form), quantity(self.resolve(entry.get("count"), 1.0)))
