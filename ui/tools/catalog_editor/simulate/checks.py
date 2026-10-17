"""Evaluates catalog conditions offline from a Situation; what can't be known becomes an UnknownCheck."""

from __future__ import annotations

import operator
import random as random_module
import re
from collections.abc import Callable, Sequence

from creation_lib.catalog.plugin_catalog import parse_form_key
from ui.tools.catalog_editor.widgets import templates
from ui.tools.catalog_editor.widgets.functions import BY_ID

COMPARE: dict[str, Callable[[float, float], bool]] = {
    "==": operator.eq, "!=": operator.ne, ">": operator.gt, ">=": operator.ge, "<": operator.lt, "<=": operator.le}
LOCATION_KEYWORD_FUNCTIONS = (562, 579, 8007)
PLAYER_SET_RUN_ONS = ("active_players", "potential_players")
NIGHT_STARTS, NIGHT_ENDS = 20.0, 6.0
TALES_NAMES = {118: "hour of day (GetCurrentTime)"}

# The answer, "random" for a per-roll chance, or None when only the author can say.
Answer = bool | str | None
RANDOM = "random"


def canon(key: str) -> str:
    parsed = parse_form_key(key) if isinstance(key, str) else None
    return f"{parsed[0]}:{parsed[1]:X}" if parsed else key


def fixed_answer(function: int) -> float | None:
    known = BY_ID.get(function)
    found = re.match(r"Always (\d)", known.note) if known else None
    if function == 846:
        return 0.0
    return float(found.group(1)) if found else None


def function_name(function: int) -> str:
    return TALES_NAMES.get(function) or (BY_ID[function].name if function in BY_ID else f"function {function}")


class Checks:
    """One preview's condition answers. `owners` maps a conditions list (by id) to its entry index."""

    def __init__(self, situation, document: dict | None = None):
        self.situation = situation
        self.document = document or {}
        self.names = {canon(key): name for key, name in getattr(situation, "names", {}).items()}
        self.rng = random_module.Random(0)
        self.unknowns: dict[str, object] = {}
        self.by_entry: dict[int | None, set[str]] = {}
        self.owners: dict[int, int | None] = {}
        self._cache: dict[int, tuple[dict, tuple[Answer, str | None]]] = {}
        self._wanted_keywords = {k.casefold() for k in situation.location_keywords} | {
            canon(k).casefold() for k in situation.location_keywords}

    def describe(self, key: str) -> str:
        return self.names.get(canon(key)) or key

    def passes(self, conditions: Sequence[dict]) -> bool:
        return self._chain(conditions, None)

    def explain(self, conditions: Sequence[dict]) -> tuple[bool, list[str]]:
        lines: list[str] = []
        return self._chain(conditions, lines), lines

    def has_random(self, conditions: Sequence[dict], seen: frozenset[str] = frozenset()) -> bool:
        for condition in conditions:
            if self._answer(condition)[0] == RANDOM:
                return True
            if condition.get("function") == 875:
                form = self._condition_form(condition)
                key = canon(((condition.get("params") or [None])[0] or {}).get("source", ""))
                if form is not None and key not in seen and self.has_random(form, seen | {key}):
                    return True
        return False

    def _chain(self, conditions: Sequence[dict], lines: list[str] | None) -> bool:
        group = False
        index = self.owners.get(id(conditions))
        for condition in conditions:
            if not group:
                group = self._pass_one(condition, index, lines)
            if not condition.get("or"):
                if not group:
                    return False
                group = False
        return not conditions or not conditions[-1].get("or") or group

    def _pass_one(self, condition: dict, index: int | None, lines: list[str] | None) -> bool:
        answer, key = self._answer(condition)
        text = self._sentence(condition)
        if answer == RANDOM:
            limit = self._compared(condition)
            passed = self.rng.randrange(100) < limit if condition.get("op") in ("<", "<=") \
                else COMPARE.get(condition.get("op", "=="), operator.eq)(self.rng.randrange(100), limit)
            mark = "?"
        elif answer is None:
            assumed = self.situation.assume.get(key)
            self._register(key, text, index)
            passed = True if assumed is None else assumed
            mark = "?" if assumed is None else ("✓" if passed else "✗")
            text += " (assumed)" if assumed is not None else " (can't tell offline; counted as passing)"
        else:
            passed = bool(answer)
            mark = "✓" if passed else "✗"
        if lines is not None:
            lines.append(f"{mark} {text}")
        return passed

    def _register(self, key: str, label: str, index: int | None) -> None:
        from ui.tools.catalog_editor.simulate import UnknownCheck

        self.unknowns.setdefault(key, UnknownCheck(key, label, index))
        self.by_entry.setdefault(index, set()).add(key)

    def _condition_form(self, condition: dict) -> list[dict] | None:
        param = (condition.get("params") or [None])[0]
        source = param.get("source", "") if isinstance(param, dict) else ""
        return (self.document.get("condition_forms") or {}).get(canon(source)) or \
            (self.document.get("condition_forms") or {}).get(source)

    def _answer(self, condition: dict) -> tuple[Answer, str | None]:
        cached = self._cache.get(id(condition))
        if cached and cached[0] is condition:
            return cached[1]
        result = self._evaluate(condition)
        self._cache[id(condition)] = (condition, result)
        return result

    def _unknown_key(self, condition: dict) -> str:
        param = (condition.get("params") or [None])[0]
        what = ""
        if isinstance(param, dict):
            what = canon(param.get("form") or param.get("source") or str(param.get("raw", "")))
        compared = canon(condition["global"]) if condition.get("global") else ""
        return (f"{condition.get('function')}:{what}:{condition.get('run_on', 'subject')}:"
                f"{condition.get('reference', '')}:{compared}")

    def _evaluate(self, condition: dict) -> tuple[Answer, str | None]:
        function = condition.get("function")
        run_on = condition.get("run_on", "subject")
        situation = self.situation
        if run_on in PLAYER_SET_RUN_ONS or (run_on == "instance_owner" and not situation.has_subject):
            return False, None
        unknown = (None, self._unknown_key(condition))
        if condition.get("unresolved") or condition.get("unsupported_flags"):
            return unknown
        compare = COMPARE.get(condition.get("op", "=="))
        value = self._compared(condition)
        if compare is None or value is None:
            return unknown
        if function == 77:
            return RANDOM, None
        left = self._left(condition, run_on)
        if left is None:
            return unknown
        return compare(left, value), None

    def _left(self, condition: dict, run_on: str) -> float | None:
        function = condition["function"]
        situation = self.situation
        if function == 80:
            return float(situation.level)
        if function == 765:
            if situation.hour is None:
                return None
            hour = situation.hour % 24
            return float(hour >= NIGHT_STARTS or hour < NIGHT_ENDS)
        if function == 849:
            return self._in_region(condition)
        if function in LOCATION_KEYWORD_FUNCTIONS:
            return float(self._location_has(condition))
        if function in (828, 837):
            return float(situation.subject_is_player) if run_on == "subject" else None
        if function == 875:
            form = self._condition_form(condition)
            return None if form is None else float(self._chain(form, None))
        return fixed_answer(function)

    def _in_region(self, condition: dict) -> float | None:
        param = (condition.get("params") or [None])[0] or {}
        key = canon(param.get("source", ""))
        region = (self.document.get("regions") or {}).get(key) or {}
        if self.situation.region and canon(self.situation.region).casefold() == key.casefold():
            return 1.0
        return None if region.get("nuke") else 0.0

    def _keyword_forms(self, condition: dict) -> set[str]:
        param = (condition.get("params") or [None])[0] or {}
        key = canon(param.get("form") or param.get("source") or "")
        return {key.casefold(), self.describe(key).casefold()}

    def _location_has(self, condition: dict) -> bool:
        return bool(self._keyword_forms(condition) & self._wanted_keywords)

    def _compared(self, condition: dict) -> float | None:
        """What the condition compares against: its global's value when it names one, None if that's unknown."""
        key = condition.get("global")
        if not key:
            return float(condition.get("value", 0))
        values = self.situation.globals
        found = values.get(key)
        found = values.get(canon(key)) if found is None else found
        return None if found is None else float(found)

    def _sentence(self, condition: dict) -> str:
        sentence = self._plain_sentence(condition)
        key = condition.get("global")
        if not key:
            return sentence
        value = self._compared(condition)
        return f"{sentence} (global {self.describe(key)} = {'?' if value is None else f'{value:g}'})"

    def _plain_sentence(self, condition: dict) -> str:
        function = condition.get("function")
        value = self._compared(condition)
        value = float(condition.get("value", 0)) if value is None else value
        op = condition.get("op", "==")
        if function == 80:
            word = {"==": "=", ">=": "≥", "<=": "≤", "!=": "≠"}.get(op, op)
            return f"level {word} {value:g} (level is {self.situation.level})"
        param = (condition.get("params") or [None])[0] or {}
        name = self.describe(param.get("form") or param.get("source") or "") if param else ""
        if function in LOCATION_KEYWORD_FUNCTIONS:
            has = self._location_has(condition)
            return f"location {'has' if has else 'lacks'} {name}"
        if function == 849:
            in_region = self._in_region(condition)
            return f"{'in' if in_region else 'not in'} region {name}"
        if function == 765:
            if self.situation.hour is None:
                return "it is night" if templates.truth(condition) else "it is not night"
            return f"it is {'night' if self._left(condition, 'subject') else 'day'} (hour {self.situation.hour})"
        if function in BY_ID or function in TALES_NAMES:
            return templates.sentence(condition, self.describe) if templates.match(condition) else \
                f"{function_name(function)}({name}) {op} {value:g}"
        return f"{function_name(function)} {op} {value:g}"
