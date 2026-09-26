from __future__ import annotations

import copy
from pathlib import Path

import pytest

from creation_lib.papyrus_lsp import ScriptDB
from creation_lib.pex.native_runtime import compile_psc, parse_pex_file_native
from creation_lib.pex.types import ValueType
from bacup_lib.tests.papyrus_support import _fo4_base_source


SOURCE = Path(__file__).resolve().parents[1] / "script_additions/fo76_fo4/B21/CurrencyQuestRewards.psc"
PROPERTIES = ("RewardStages", "RewardStageItems", "RewardItems", "RewardAmounts", "RewardMultipliers", "HoldingLimits")


@pytest.fixture(scope="module")
def script(tmp_path_factory):
    root = tmp_path_factory.mktemp("currency-rewards")
    result = compile_psc(SOURCE.read_text(), imports=[str(_fo4_base_source())], game="fo4", source_path=str(SOURCE))
    assert result.ok, result.diagnostics
    path = root / "CurrencyQuestRewards.pex"
    path.write_bytes(result.pex_bytes)
    return parse_pex_file_native(path).objects[0]


def test_bound_properties_and_events_resolve(tmp_path):
    db = ScriptDB(str(tmp_path / "lsp.db"), source_dirs=[str(SOURCE.parent.parent)])
    try:
        for name in PROPERTIES:
            assert db.has_property("B21:CurrencyQuestRewards", name)
        assert db.has_event("B21:CurrencyQuestRewards", "OnStageSet")
        assert db.has_event("B21:CurrencyQuestRewards", "OnQuestInit")
        assert not db.has_property("B21:CurrencyQuestRewards", "InventedReward")
    finally:
        db.close()


def test_existing_saves_reload_the_item_bindings_with_the_stage_array(tmp_path):
    source = SOURCE.with_name("QuestRewards.psc")
    result = compile_psc(source.read_text(), imports=[str(_fo4_base_source())], game="fo4", source_path=str(source))
    assert result.ok, result.diagnostics
    path = tmp_path / "QuestRewards.pex"
    path.write_bytes(result.pex_bytes)
    variables = {v.name.lower(): v for v in parse_pex_file_native(path).objects[0].variables}
    for name in ("ItemStages", "RewardItems", "RewardCounts"):
        assert variables[f"::{name.lower()}_var"].is_const


def test_reward_rows_read_the_condition_sets_the_conversion_writes(tmp_path):
    source = SOURCE.with_name("QuestRewards.psc")
    result = compile_psc(source.read_text(), imports=[str(_fo4_base_source())], game="fo4", source_path=str(source))
    assert result.ok, result.diagnostics
    path = tmp_path / "QuestRewards.pex"
    path.write_bytes(result.pex_bytes)
    script = parse_pex_file_native(path).objects[0]
    variables = {v.name.lower(): v for v in script.variables}
    # Names match the properties repair_quest_completion_rewards emits.
    for name in (
        "XPConditionSets",
        "CapsConditionSets",
        "ItemConditionSets",
        "ConditionSetStarts",
        "ConditionSetCounts",
        "ConditionKinds",
        "ConditionOperators",
        "ConditionForms",
        "ConditionValues",
    ):
        assert variables[f"::{name.lower()}_var"].is_const
    functions = {f.name.lower() for s in script.states for f in s.functions}
    assert {"rowpasses", "conditionsetpasses", "conditionpasses", "compare"} <= functions


class RewardVM:
    # Run the compiled instructions with inventory and GlobalVariable natives mocked.
    def __init__(self, script, *, amount=8, owned=0, limit=2_147_483_647, multiplier=1):
        self.functions = {f.name.lower(): f for s in script.states for f in s.functions}
        self.state = {v.name.lower(): v.data.data for v in script.variables}
        values = ([2000], [1], ["scrip"], ["amount"], [multiplier], [limit])
        self.state.update({f"::{key.lower()}_var": value for key, value in zip(PROPERTIES, values)})
        self.globals = {"amount": amount}
        self.inventory = {"scrip": owned}
        self.added = []

    def run(self, name, *parameters):
        function = self.functions[name.lower()]
        local = {p.name.lower(): value for p, value in zip(function.params, parameters)}
        local["self"] = self

        def value(arg):
            if arg.type != ValueType.IDENTIFIER:
                return arg.data
            key = arg.data.lower()
            return local[key] if key in local else self.state[key]

        def assign(arg, result):
            key = arg.data.lower()
            (self.state if key in self.state else local)[key] = result

        pc = 0
        for _ in range(10000):
            if pc == len(function.instructions):
                return
            instruction = function.instructions[pc]
            op, a = instruction.opcode.name, instruction.args
            if op == "RETURN":
                return value(a[0])
            if op == "JMP":
                pc += value(a[0])
                continue
            if op in {"JMPF", "JMPT"}:
                if bool(value(a[0])) == (op == "JMPT"):
                    pc += value(a[1])
                    continue
            elif op in {"ASSIGN", "CAST"}:
                assign(a[0], value(a[1]))
            elif op == "NOT":
                assign(a[0], not value(a[1]))
            elif op.startswith("CMP_"):
                left, right = value(a[1]), value(a[2])
                compare = {"CMP_EQ": lambda: left == right, "CMP_GT": lambda: left > right,
                           "CMP_LT": lambda: left < right, "CMP_LTE": lambda: left <= right}
                assign(a[0], compare[op]())
            elif op in {"IADD", "ISUB", "IMUL", "IDIV"}:
                left, right = value(a[1]), value(a[2])
                result = {"IADD": lambda: left + right, "ISUB": lambda: left - right,
                          "IMUL": lambda: left * right, "IDIV": lambda: left // right}[op]()
                assert -2_147_483_648 <= result <= 2_147_483_647
                assign(a[0], result)
            elif op == "ARRAY_CREATE":
                assign(a[0], [False] * value(a[1]))
            elif op == "ARRAY_LENGTH":
                assign(a[0], len(value(a[1])))
            elif op == "ARRAY_GETELEMENT":
                assign(a[0], value(a[1])[value(a[2])])
            elif op == "ARRAY_SETELEMENT":
                value(a[0])[value(a[1])] = value(a[2])
            elif op == "CALLSTATIC":
                assert [arg.data.lower() for arg in a[:2]] == ["game", "getplayer"]
                assign(a[2], "player")
            elif op == "CALLMETHOD":
                method, owner = a[0].data.lower(), value(a[1])
                args = [value(arg) for arg in a[4:]]
                assert len(args) == value(a[3])
                if owner is self:
                    result = self.run(method, *args)
                elif method == "getvalueint":
                    result = self.globals[owner]
                elif method == "getitemcount":
                    assert owner == "player"
                    result = self.inventory.get(args[0], 0)
                elif method == "additem":
                    assert owner == "player" and args[1] > 0
                    self.inventory[args[0]] = self.inventory.get(args[0], 0) + args[1]
                    self.added.append(args[:2])
                    result = None
                else:
                    raise AssertionError(method)
                assign(a[2], result)
            else:
                raise AssertionError(op)
            pc += 1
        raise AssertionError("reward script did not terminate")


def test_stage_items_duplicate_events_saved_state_and_new_quest_run(script):
    vm = RewardVM(script)
    vm.run("OnQuestInit")
    vm.run("OnStageSet", 1999, 1)
    vm.run("OnStageSet", 2000, 0)
    assert vm.added == []
    vm.run("OnStageSet", 2000, 1)
    assert vm.added == [["scrip", 8]]
    restored = RewardVM(script)
    restored.state = copy.deepcopy(vm.state)
    restored.run("OnStageSet", 2000, 1)
    assert restored.added == []
    vm.run("OnStageSet", 2000, 1)
    assert len(vm.added) == 1
    vm.globals["amount"] = 11
    vm.run("OnQuestInit")
    vm.run("OnStageSet", 2000, 1)
    assert vm.added[-1] == ["scrip", 11]


@pytest.mark.parametrize("amount,owned,limit,multiplier,expected", [
    (8, 11000, 2_147_483_647, 1, 8), (500, 9995, 10000, 1, 5),
    (500, 10000, 10000, 1, 0), (0, 0, 10000, 1, 0), (-1, 0, 10000, 1, 0),
    (2_147_483_647, 0, 2_147_483_647, 3, 2_147_483_647),
    (8, 2_147_483_645, 2_147_483_647, 1, 2), (3, 0, 10000, 2, 6),
])
def test_currency_limits_and_safe_multiplication(script, amount, owned, limit, multiplier, expected):
    vm = RewardVM(script, amount=amount, owned=owned, limit=limit, multiplier=multiplier)
    vm.run("OnStageSet", 2000, 1)
    assert sum(count for _, count in vm.added) == expected


def test_three_currency_rows_and_literal_quantity(script):
    vm = RewardVM(script)
    values = ([2000]*3, [1, 2, 3], ["scrip", "notes", "bullion"], ["amount", None, "gold"], [1, 3, 1], [2_147_483_647, 2_147_483_647, 10000])
    vm.state.update({f"::{key.lower()}_var": value for key, value in zip(PROPERTIES, values)})
    vm.globals["gold"] = 500
    for item in [1, 2, 3, 2, 1, 3]:
        vm.run("OnStageSet", 2000, item)
    assert vm.added == [["scrip", 8], ["notes", 3], ["bullion", 500]]
