"""The offline leveled-list roll: shared vectors with Tales' RollTests.cpp, odds, unknown checks and fallback."""
from __future__ import annotations

import json
from collections import deque
from pathlib import Path

import pytest

from ui.tools.catalog_editor.simulate import Situation, preview_list, sample
from ui.tools.catalog_editor.simulate import roll

VECTORS = (Path(__file__).resolve().parents[2] / "mods" / "B21_TalesFromAppalachia" / "tests" / "leveled-lists"
           / "roll_vectors.json")
CASES = json.loads(VECTORS.read_text(encoding="utf-8"))["cases"]


class Scripted:
    def __init__(self, integers, units):
        self.integers, self.units = deque(integers), deque(units)
        self.integer_calls = self.unit_calls = 0

    def integer(self, minimum, maximum_exclusive):
        self.integer_calls += 1
        value = self.integers.popleft()
        assert minimum <= value < maximum_exclusive
        return value

    def unit(self):
        self.unit_calls += 1
        return self.units.popleft()


class FakeHost:
    def __init__(self, case):
        self.case = case

    def has_subject(self):
        return self.case["subject"]

    def passes(self, conditions):
        return all(condition["function"] != 999 for condition in conditions)

    def global_value(self, key):
        return self.case["globals"].get(key)

    def list_passed(self, level):
        return self.case.get("level_after_pass") or level


@pytest.mark.parametrize("case", CASES, ids=[case["name"] for case in CASES])
def test_shared_vectors(case):
    random = Scripted(case["integers"], case["units"])
    curves = {key: row["points"] for key, row in case.get("curves", {}).items()}
    roller = roll.Roller(curves, case.get("delegates", {}), FakeHost(case), random)
    result = roller.roll(case["list"], case["level"], case["count"], roll.FORCES[case["force"]])
    expect = case["expect"]
    assert [pick.form for pick in result.picks] == expect["forms"]
    assert random.integer_calls == expect["integer_calls"]
    assert random.unit_calls == expect["unit_calls"]
    assert result.max_count == expect["max_count"]
    if "quantities" in expect:
        assert [pick.quantity for pick in result.picks] == expect["quantities"]
    if "multiplier" in expect:
        assert result.multiplier == expect["multiplier"]


def row(*entries, flags=0, **extra):
    return {"target": "T.esp:000800", "flags": flags, "entries": list(entries), **extra}


def item(form, level=1, chance=0, conditions=None, **extra):
    out = {"form": form, "level": {"value": level}, "chance_none": {"value": chance}, **extra}
    if conditions:
        out["conditions"] = conditions
    return out


def condition(function, value=1.0, op="==", params=None, **extra):
    return {"function": function, "op": op, "value": value, "run_on": "subject", "params": params or [None, None],
            **extra}


def keyword(function, key):
    return condition(function, params=[{"source": key, "form": key}, None])


def test_single_pick_odds_are_exact():
    preview = preview_list(row(item("A.esp:1"), item("A.esp:2", chance=50), item("A.esp:3", level=30)),
                           Situation(level=25))
    odds = [outcome.odds for outcome in preview.entries]
    assert odds == [pytest.approx(0.5), pytest.approx(0.5 * 0.49), None]
    assert preview.entries[2].reasons == ["✗ roll level 25 < 30"]
    assert not preview.falls_back and preview.chance_none == 0


def test_list_chance_none_and_weights():
    weighted = row(item("A.esp:1", chance=1), item("A.esp:2", chance=3), flags=0x200,
                   chance_none={"value": 20})
    odds = [o.odds for o in preview_list(weighted, Situation()).entries]
    # Rolls 0..3 of 4: A takes 0 and 1, B takes 2 and 3.
    assert odds == [pytest.approx(0.8 * 2 / 4), pytest.approx(0.8 * 2 / 4)]
    assert preview_list(weighted, Situation()).chance_none == pytest.approx(0.2)


def test_use_all_odds_are_independent():
    preview = preview_list(row(item("A.esp:1"), item("A.esp:2", chance=50), flags=0x04), Situation())
    assert [o.odds for o in preview.entries] == [pytest.approx(1.0), pytest.approx(0.49)]


def test_exact_odds_match_the_monte_carlo():
    entries = row(item("A.esp:1"), item("A.esp:2", chance=25), item("A.esp:3", level=5))
    preview = preview_list(entries, Situation(level=10))
    counts = sample(entries, Situation(level=10), 20000, seed=3)
    for outcome, form in zip(preview.entries, ("A.esp:1", "A.esp:2", "A.esp:3")):
        assert counts.get(form, 0) / 20000 == pytest.approx(outcome.odds or 0, abs=0.02)


def test_sample_is_seeded_and_counts_nothing_as_empty_key():
    entries = row(item("A.esp:1", chance=50))
    first = sample(entries, Situation(), 500, seed=7)
    assert first == sample(entries, Situation(), 500, seed=7)
    assert first[""] + first["A.esp:1"] == 500 and first[""] > 150


def test_child_lists_expand_in_sampling():
    child = row(item("A.esp:10"), item("A.esp:11"))
    parent = row(item("A.esp:2"))
    counts = sample(parent, Situation(), 400, resolve_list=lambda key: child if key == "A.esp:2" else None)
    assert set(counts) == {"A.esp:10", "A.esp:11"}
    preview = preview_list(parent, Situation(), lambda key: child if key == "A.esp:2" else None)
    assert preview.entries[0].odds == pytest.approx(1.0, abs=0.01)


def test_out_of_scope_list_is_left_to_fallout_4():
    preview = preview_list(row(item("A.esp:1")), Situation(in_seventysix=False))
    assert not preview.used_by_tales and preview.entries[0].odds is None
    assert preview_list({**row(item("A.esp:1")), "scope": "always"}, Situation(in_seventysix=False)).used_by_tales
    assert sample(row(item("A.esp:1")), Situation(in_seventysix=False), 10) == {}


def test_level_and_location_keyword_checks_are_evaluated():
    low = item("A.esp:1", conditions=[condition(80, 20, ">=")])
    camp = item("A.esp:2", conditions=[keyword(562, "A.esp:900")])
    situation = Situation(level=15, location_keywords=("A.esp:0900",), names={"A.esp:900": "LocTypeRaiderCamp"})
    preview = preview_list(row(low, camp), situation)
    assert [o.eligible for o in preview.entries] == [False, True]
    assert preview.entries[0].reasons[0].startswith("✗ level ≥ 20")
    assert "✓ location has LocTypeRaiderCamp" in preview.entries[1].reasons
    other = preview_list(row(low, camp), Situation(level=25))
    assert other.entries[0].eligible and not other.entries[1].eligible
    assert other.entries[1].reasons == ["✗ location lacks A.esp:900"]


def test_night_and_region_use_the_situation():
    night = item("A.esp:1", conditions=[condition(765)])
    region = {"A.esp:700": {"world": "W", "polygons": [[[0, 0]]]}}
    inside = item("A.esp:2", conditions=[condition(849, params=[{"source": "A.esp:700", "form": "A.esp:700"}, None])])
    document = {"regions": region}
    both = lambda **kw: preview_list(row(night, inside), Situation(document=document, **kw))
    assert [o.eligible for o in both(hour=23, region="A.esp:700").entries] == [True, True]
    assert [o.eligible for o in both(hour=12).entries] == [False, False]
    assert both(hour=12).unknowns == []


def test_unknown_checks_count_as_passing_until_assumed():
    quest = condition(543, params=[{"source": "A.esp:500", "form": "A.esp:500"}, None])
    entries = row(item("A.esp:1", conditions=[quest]), item("A.esp:2"))
    preview = preview_list(entries, Situation())
    [unknown] = preview.unknowns
    assert unknown.entry_index == 0 and preview.entries[0].unknowns == [unknown.key]
    assert preview.entries[0].eligible and "counted as passing" in preview.entries[0].reasons[0]
    assumed = preview_list(entries, Situation(assume={unknown.key: False}))
    assert not assumed.entries[0].eligible and assumed.entries[1].odds == pytest.approx(1.0)
    assert assumed.entries[0].reasons[0].startswith("✗") and assumed.unknowns[0].key == unknown.key


def test_one_unanswerable_check_sends_the_whole_list_to_fallout_4():
    rifle = item("A.esp:6", conditions=[condition(118, 18, ">=")], name="Ultracite Laser Rifle")
    preview = preview_list(row(item("A.esp:1"), rifle), Situation())
    assert preview.falls_back
    assert preview.fallback_reasons == [
        "Item 2 (Ultracite Laser Rifle): hour of day (GetCurrentTime, 18) can't be answered by Tales"]
    assert preview.entries[0].odds is not None


def test_list_checks_failing_empties_every_entry():
    preview = preview_list(row(item("A.esp:1"), conditions=[condition(80, 99, ">=")]), Situation(level=5))
    assert preview.entries[0].odds is None
    assert preview_list(row(item("A.esp:1"), conditions=[condition(80, 99, ">=")]),
                        Situation(level=5, has_subject=False)).entries[0].odds == pytest.approx(1.0)


def test_conditions_compared_against_a_global_use_its_value():
    gated = condition(80, 0, ">=", **{"global": "A.esp:700"})
    entries = row(item("A.esp:1", conditions=[gated]), item("A.esp:2"))
    unknown = preview_list(entries, Situation(level=15))
    assert unknown.entries[0].eligible and unknown.entries[0].unknowns
    known = preview_list(entries, Situation(level=15, globals={"A.esp:700": 20.0}))
    assert not known.entries[0].eligible and not known.unknowns
    assert "= 20" in known.entries[0].reasons[0]


def test_random_percent_against_a_global_rolls_its_value():
    chance = condition(77, 0, "<=", **{"global": "A.esp:701"})
    entries = row(item("A.esp:1", conditions=[chance]))
    counts = sample(entries, Situation(globals={"A.esp:701": 25.0}), 4000, seed=1)
    assert counts["A.esp:1"] / 4000 == pytest.approx(0.25, abs=0.03)


def test_global_values_read_through_the_form_source():
    from ui.tools.catalog_editor.features.leveled_form import GlobalValues

    class Forms:
        def global_value(self, key):
            return {"A.esp:700": 3.0}.get(key)

    values = GlobalValues(Forms())
    assert values["A.esp:700"] == 3.0 and "A.esp:700" in values and values.get("A.esp:999") is None
    assert "A.esp:999" not in values and list(values) == ["A.esp:700"]
