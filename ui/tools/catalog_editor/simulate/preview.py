"""preview_list and sample: Tales' leveled-list roll run offline against a Situation."""

from __future__ import annotations

import random as random_module
from collections.abc import Sequence

from ui.tools.catalog_editor.simulate import roll as rolls
from ui.tools.catalog_editor.simulate.checks import Checks, canon, function_name

MAX_DEPTH = 32
MONTE_CARLO_ROLLS = 4000
VALUE_KEYS = (("level", "level"), ("count", "count"), ("chance_none", "chance of nothing"))


class _Random:
    def __init__(self, source: random_module.Random):
        self.source = source

    def integer(self, minimum: int, maximum_exclusive: int) -> int:
        return self.source.randrange(minimum, maximum_exclusive)

    def unit(self) -> float:
        return self.source.random()


class _Host:
    def __init__(self, checks: Checks):
        self.checks = checks

    def has_subject(self) -> bool:
        return self.checks.situation.has_subject

    def passes(self, conditions: Sequence[dict]) -> bool:
        return self.checks.passes(conditions)

    def global_value(self, key: str) -> float | None:
        values = self.checks.situation.globals
        return values[key] if key in values else values.get(canon(key))

    def list_passed(self, level: int) -> int:
        return level


class Simulation:
    def __init__(self, situation, resolve_list=None):
        self.situation = situation
        self.document = situation.document
        self.checks = Checks(situation, self.document)
        self.resolve_list = resolve_list
        self.random = _Random(self.checks.rng)
        self.curves = _keyed(self.document.get("curves"), lambda row: row.get("points") or [])
        self.delegates = _keyed(self.document.get("delegates"), lambda row: row)
        self.host = _Host(self.checks)

    def own_conditions(self, row: dict) -> None:
        for index, entry in enumerate(row.get("entries") or []):
            if entry.get("conditions"):
                self.checks.owners[id(entry["conditions"])] = index

    def in_scope(self, row: dict) -> bool:
        return row.get("scope") == "always" or self.situation.in_seventysix

    def roller(self) -> rolls.Roller:
        return rolls.Roller(self.curves, self.delegates, self.host, self.random)

    def child(self, form: str) -> dict | None:
        row = self.resolve_list(form) if self.resolve_list else None
        return row if row is not None and self.in_scope(row) else None

    def roll_once(self, row: dict, count: int = 1, depth: int = 1) -> tuple[list[rolls.Pick], list[str]]:
        roller = self.roller()
        result = roller.roll(row, self.situation.level, count)
        leaves: list[str] = []

        def emit(pick: rolls.Pick) -> int:
            child = self.child(pick.form) if depth < MAX_DEPTH else None
            if child is None:
                leaves.append(pick.form)
            else:
                leaves.extend(self.roll_once(child, pick.quantity, depth + 1)[1])
            return len(leaves)

        kept = rolls.emit_use_all(result.picks, result.max_count, emit)
        return result.picks, leaves[:kept]

    def sample(self, row: dict, count: int) -> tuple[dict[str, int], list[int]]:
        """Roll `count` times: forms by number of rolls they appeared in, and top-level entry hits."""
        forms: dict[str, int] = {}
        hits = [0] * len(row.get("entries") or [])
        entries = {id(entry): index for index, entry in enumerate(row.get("entries") or [])}
        for _ in range(count):
            picks, leaves = self.roll_once(row)
            for index in {entries[id(pick.entry)] for pick in picks}:
                hits[index] += 1
            for form in (set(leaves) or {""}):
                forms[form] = forms.get(form, 0) + 1
        return forms, hits


def _keyed(section: dict | None, convert) -> dict:
    out = {}
    for key, row in (section or {}).items():
        out[key] = out[canon(key)] = convert(row)
    return out


def _percent(value: float) -> float:
    return rolls.chance(value) / 100


def _exact_odds(sim: Simulation, row: dict, candidates: list[dict], eligible: list[bool]) -> list[float]:
    flags = int(row.get("flags", 0))
    roller = sim.roller()
    entries = row.get("entries") or []
    none = _percent(roller.resolve(row.get("chance_none")))
    odds = [0.0] * len(entries)
    if flags & rolls.USE_ALL and not flags & rolls.EACH_IN_COUNT:
        for index, entry in enumerate(entries):
            if eligible[index]:
                percent = rolls.chance(roller.resolve(entry.get("chance_none")))
                odds[index] = (1 - none) * (1 - min(1.0, (percent + 1) / 100) if percent else 1.0)
        return odds
    positions = {id(entry): index for index, entry in enumerate(entries)}
    if not candidates:
        return odds
    if flags & rolls.WEIGHTED:
        weights = [roller.weight(entry) for entry in candidates]
        total = sum(weights)
        if total == 0:
            odds[positions[id(candidates[0])]] = 1 - none
            return odds
        low = 0
        for position, (entry, weight) in enumerate(zip(candidates, weights)):
            high = low + weight
            first = 0 if position == 0 else low + 1
            odds[positions[id(entry)]] = (1 - none) * max(0, min(high, total - 1) - first + 1) / total
            low = high
        return odds
    for entry in candidates:
        percent = rolls.chance(roller.resolve(entry.get("chance_none")))
        stays = 1 - min(1.0, (percent + 1) / 100) if percent else 1.0
        odds[positions[id(entry)]] = (1 - none) * stays / len(candidates)
    return odds


def _item_label(entry: dict, index: int, checks: Checks) -> str:
    name = entry.get("name") or entry.get("editor_id") or checks.describe(entry.get("form", ""))
    return f"Item {index + 1} ({name})"


def _fallback_reasons(row: dict, situation) -> list[str]:
    from ui.tools.catalog_editor.features.leveled_lists import fallback_findings

    checks = Checks(situation, situation.document)
    entries = row.get("entries") or []
    out = []
    for finding in fallback_findings(row, situation.document, name_of=checks.describe):
        item = finding.item
        in_range = item is not None and item < len(entries)
        subject = _item_label(entries[item], item, checks) if in_range else "The list" if item is None else finding.where
        rows = (entries[item].get("conditions") if in_range else row.get("conditions")) or []
        condition = rows[finding.check] if finding.check is not None and finding.check < len(rows) else None
        reason = finding.reason
        if condition and reason == "Tales doesn't answer this function":
            function = condition.get("function")
            value = condition.get("value")
            detail = f", {value:g}" if isinstance(value, (int, float)) else ""
            name = function_name(function)
            name = f"{name[:-1]}{detail})" if name.endswith(")") else f"{name} ({function}{detail})"
            reason = f"{name} can't be answered by Tales"
        out.append(f"{subject}: {reason}")
    return out


def preview_list(row: dict, situation, resolve_list=None):
    from ui.tools.catalog_editor.simulate import EntryOutcome, ListPreview

    sim = Simulation(situation, resolve_list)
    checks = sim.checks
    entries = row.get("entries") or []
    flags = int(row.get("flags", 0))
    fallback = _fallback_reasons(row, situation)
    used = sim.in_scope(row)
    outcomes = [EntryOutcome(index, None, False) for index in range(len(entries))]
    preview = ListPreview(used, bool(fallback) and used, fallback, 0.0, outcomes, [])
    if not used:
        for outcome in outcomes:
            outcome.reasons.append("✗ this list only applies in the SeventySix world; Fallout 4 rolls the plugin record")
        return preview

    sim.own_conditions(row)
    roller = sim.roller()
    level = max(situation.level, 1)
    preview.chance_none = _percent(roller.resolve(row.get("chance_none")))
    if fallback:
        preview.notes.append("Tales rolls this list through Fallout 4; the odds below are what the FO76 roll would give")

    list_conditions = row.get("conditions") or []
    if list_conditions and not situation.has_subject:
        preview.notes.append("The list's own checks are skipped: nothing is rolling for it")
    elif list_conditions:
        passed, lines = checks.explain(list_conditions)
        preview.notes += [f"List check {line}" for line in lines]
        if not passed:
            for outcome in outcomes:
                outcome.reasons.append("✗ the list's own checks fail, so nothing drops")
            preview.unknowns = list(checks.unknowns.values())
            return preview

    use_all = bool(flags & rolls.USE_ALL) and not flags & rolls.EACH_IN_COUNT
    candidates = [] if use_all else roller.candidates(row, level, rolls.FORCE_DEFAULT)
    chosen = {id(entry) for entry in candidates}
    first_match_done = False
    eligible = []
    for index, entry in enumerate(entries):
        outcome = outcomes[index]
        entry_level = roller.level_of(entry)
        passed = True
        if entry_level > level:
            outcome.reasons.append(f"✗ roll level {level} < {entry_level}")
            passed = False
        else:
            if entry_level > 1:
                outcome.reasons.append(f"✓ roll level {level} ≥ {entry_level}")
            if entry.get("conditions") and not (flags & rolls.FIRST_MATCH and first_match_done):
                passed, lines = checks.explain(entry["conditions"])
                outcome.reasons += lines
        for key, label in VALUE_KEYS:
            value = entry.get(key) or {}
            if value.get("global") and sim.host.global_value(value["global"]) is None:
                outcome.reasons.append(f"? {label} comes from global {checks.describe(value['global'])}, "
                                       f"which has no value here; using {value.get('value', 0):g}")
        ok = passed if use_all else id(entry) in chosen
        if not use_all and passed and not ok:
            outcome.reasons.append("✗ an earlier entry already matched" if flags & rolls.FIRST_MATCH
                                   else "✗ a higher level band is used instead")
        if flags & rolls.FIRST_MATCH and id(entry) in chosen:
            first_match_done = True
        outcome.eligible = ok
        eligible.append(ok)
        outcome.unknowns = sorted(checks.by_entry.get(index, ()))

    exact = (not _has_child(sim, entries) and not any(checks.has_random(e.get("conditions") or []) for e in entries)
             and not checks.has_random(list_conditions)
             and not (use_all and rolls.byte(roller.resolve(row.get("max_count"))) != 0))
    if exact:
        odds = _exact_odds(sim, row, candidates, eligible)
    else:
        preview.notes.append(f"Odds come from {MONTE_CARLO_ROLLS} simulated rolls")
        _, hits = sim.sample(row, MONTE_CARLO_ROLLS)
        odds = [hit / MONTE_CARLO_ROLLS for hit in hits]
    for outcome, value in zip(outcomes, odds):
        outcome.odds = value if outcome.eligible else None
    preview.unknowns = list(checks.unknowns.values())
    return preview


def _has_child(sim: Simulation, entries: list[dict]) -> bool:
    return any(sim.child(sim.delegates.get(e.get("form", ""), e.get("form", ""))) is not None for e in entries)


def sample(row: dict, situation, count: int, seed: int = 0, resolve_list=None) -> dict[str, int]:
    sim = Simulation(situation, resolve_list)
    sim.checks.rng.seed(seed)
    if not sim.in_scope(row):
        return {}
    sim.own_conditions(row)
    return sim.sample(row, count)[0]
