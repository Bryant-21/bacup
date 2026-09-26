from __future__ import annotations

import csv
from pathlib import Path

import yaml

from bacup_lib.workflows.unified import _script_patch_source, _script_addition_sources


REPO_ROOT = Path(__file__).resolve().parents[5]
STATUS = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"
CONTRACT = "contracts/unsupported-fo76-recipe-learning.md"
TRANSLATION_MAP = (
    REPO_ROOT
    / "bacup"
    / "py_bacup_lib"
    / "native"
    / "conversion"
    / "src"
    / "embedded"
    / "translation_maps"
    / "fo76_to_fo4.yaml"
)
EXCLUDED_SCRIPTS = {
    "DefaultTeachRecipeOnRead.psc": "DefaultTeachRecipeOnRead",
    "Economy/ScorchbeastRecipesScript.psc": "Economy:ScorchbeastRecipesScript",
    "Economy/UnlockTaxidermyRecipesScript.psc": (
        "Economy:UnlockTaxidermyRecipesScript"
    ),
}


def _status() -> dict[str, dict[str, str]]:
    with STATUS.open(encoding="utf-8", newline="") as stream:
        return {row["relative_path"]: row for row in csv.DictReader(stream)}


def test_recipe_learning_scripts_are_explicitly_unsupported_and_unpatched() -> None:
    status = _status()

    for path, script_name in EXCLUDED_SCRIPTS.items():
        assert status[path]["terminal_state"] == "unsupported-online"
        assert status[path]["evidence"] == CONTRACT
        assert _script_patch_source(script_name) is None


def test_native_recipe_fields_are_replaced_by_fo4_adapter() -> None:
    translation = yaml.safe_load(TRANSLATION_MAP.read_text(encoding="utf-8"))
    dropped = set(translation["COBJ"]["drop"])

    assert {
        "LearnMethod",
        "LearnRecipeFrom",
        "ConstructibleInstantiationFilterKeyword",
    } <= dropped
    additions = _script_addition_sources("fo76", "fo4")
    assert additions["b21:planlearnonread"][0] == "B21:PlanLearnOnRead"
