from __future__ import annotations

import json
from pathlib import Path


CURVES = {
    3: "misc/curvetables/json/misc/expeditions/xpd_ac_slotmachinechances_3tumbler.json",
    5: "misc/curvetables/json/misc/expeditions/xpd_ac_slotmachinechances_5tumbler.json",
}

# Verified from SeventySix.esm VMAD plus the English ActivateTextOverride.  Reward
# amounts are deliberately absent: the FO76 client receives those from its server.
MACHINES = (
    ("8554FE", "ATX_CAMP_Furniture_AtomicRouletteTable", 0, 50, 0),
    ("733848", "XPD_AC_RouletteTable", 0, 50, 0),
    ("6E58B4", "XPD_AC_Workshop_RouletteTable", 0, 50, 0),
    ("6EB413", "XPD_AC_HorseRacingMachine", 1, 100, 0),
    ("6D3A4E", "XPD_AC_Workshop_HorseRacingMachine", 1, 100, 0),
    ("851052", "ATX_CAMP_Furniture_AtomicDiceTable", 2, 50, 0),
    ("733849", "XPD_AC_DiceTable", 2, 50, 0),
    ("6E58B5", "XPD_AC_Workshop_DiceTable", 2, 50, 0),
    ("850EB0", "ATX_CAMP_AtomicRodeo_SlotMachine", 3, 10, 3),
    ("73C292", "XPD_AC_HoneyPotOGold_SlotMachine", 3, 10, 3),
    ("6F5195", "ATX_HoneyPotOGold_SlotMachine", 3, 10, 3),
    ("729BAF", "XPD_AC_SlotMachine1", 3, 10, 3),
    ("6D3A4F", "XPD_AC_Workshop_SlotMachine1", 3, 10, 3),
    ("61E026", "WestVirginia_SlotMachine", 3, 10, 3),
    ("729BAD", "XPD_AC_SlotMachine2", 4, 25, 3),
    ("6DAACC", "XPD_AC_Workshop_SlotMachine2", 4, 25, 3),
    ("729BB0", "XPD_AC_WorldsBiggestSlotMachine", 5, 100, 5),
    ("6D3A50", "XPD_AC_Workshop_WorldsBiggestSlotMachine", 5, 100, 5),
)


def build_casino_catalog(source_root: Path) -> dict:
    root = Path(source_root)
    curves = {}
    for tumblers, relative in CURVES.items():
        points = json.loads((root / relative).read_text(encoding="utf-8"))["curve"]
        thresholds = [float(point["y"]) for point in points]
        if [int(point["x"]) for point in points] != list(range(len(points))):
            raise ValueError(f"Casino curve {relative} has non-contiguous result indices")
        if not thresholds or thresholds[-1] != 1.0 or thresholds != sorted(thresholds):
            raise ValueError(f"Casino curve {relative} is not a cumulative distribution")
        curves[str(tumblers)] = thresholds
    return {
        "schema_version": 1,
        "source_plugin": "SeventySix.esm",
        "machines": [
            {"form_id": form_id, "editor_id": editor_id, "game_type": game_type,
             "entry_caps": cost, "tumblers": tumblers}
            for form_id, editor_id, game_type, cost, tumblers in MACHINES
        ],
        "slot_cumulative_result_thresholds": curves,
        "result_shapes": {
            "3": [
                {"result_type": 0, "winning_symbol_count": 0, "winning_symbol": 0},
                {"result_type": 1, "winning_symbol_count": 1, "winning_symbol": 1},
                {"result_type": 1, "winning_symbol_count": 2, "winning_symbol": 1},
                *({"result_type": 2, "winning_symbol_count": 3, "winning_symbol": symbol}
                  for symbol in (1, 2, 3, 4, 5, 0)),
            ],
            "5": [
                {"result_type": 0, "winning_symbol_count": 0, "winning_symbol": 0},
                {"result_type": 1, "winning_symbol_count": 1, "winning_symbol": 1},
                {"result_type": 1, "winning_symbol_count": 2, "winning_symbol": 1},
                *({"result_type": 2, "winning_symbol_count": count, "winning_symbol": symbol}
                  for count in (3, 4) for symbol in (1, 2, 3, 4, 5)),
                {"result_type": 2, "winning_symbol_count": 5, "winning_symbol": 0},
            ],
        },
        "payout_provenance": "local_policy_not_fo76_parity",
        "unsupported_source_data": [
            "FO76 server table-game odds and payouts",
            "FO76 server XPD slot cap/item rewards",
        ],
    }
