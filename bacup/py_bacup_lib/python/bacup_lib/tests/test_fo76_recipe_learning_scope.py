from __future__ import annotations

import csv
from pathlib import Path

import yaml

from bacup_lib.workflows.unified import _script_patch_source, _script_addition_sources


REPO_ROOT = Path(__file__).resolve().parents[5]
STATUS = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"
TODO = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "TODO.md"
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
NATIVE_ROOT = (
    REPO_ROOT / "bacup" / "py_bacup_lib" / "native" / "conversion" / "src"
)
TFA_COBJS = (
    REPO_ROOT / "mods" / "B21_TalesFromAppalachia" / "yaml" / "records" / "COBJ"
)
FO4_WORKSHOP_BENCHES = {"05A0C8", "05B5E3", "08280B", "05A0CA", "12E2C8", "246F85"}
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


def test_recipe_learning_is_not_tracked_as_deferred_implementation_work() -> None:
    todo = TODO.read_text(encoding="utf-8")

    assert "category=ONLINE-RECIPE" not in todo
    assert "identify an evidenced FO4 recipe-unlock substitute" not in todo


def test_plan_learning_keeps_workshop_menu_categories() -> None:
    target_hook = (
        NATIVE_ROOT / "translator" / "target_hooks" / "fo4.rs"
    ).read_text(encoding="utf-8")
    filters = (NATIVE_ROOT / "fixups" / "strip_crafting_recipe_filters.rs").read_text(
        encoding="utf-8"
    )
    catalog = (NATIVE_ROOT / "fixups" / "apply_fo76_workshop_catalog.rs").read_text(
        encoding="utf-8"
    )
    dead_gates = (NATIVE_ROOT / "fixups" / "strip_dead_workshop_conditions.rs").read_text(
        encoding="utf-8"
    )
    pipeline = (NATIVE_ROOT / "store2" / "fixups_v2.rs").read_text(encoding="utf-8")

    assert "fn book_dnam_removes_is_recipe_flag()" in target_hook
    assert "Workshop recipes lose their filters too" in filters
    assert "strips_filter_from_workshop_recipe" in filters
    assert "normalize_category_keyword" in catalog
    assert "StripCraftingRecipeFiltersVisitor" in pipeline
    assert "StripDeadWorkshopConditionsVisitor" in pipeline
    assert "DEAD_KNOWLEDGE_ACTOR_VALUES" in dead_gates
    assert "RestoreFo76PlanLearningFixup" in pipeline


def test_tales_plan_outputs_keep_workshop_recipe_categories() -> None:
    generated = [
        path
        for path in TFA_COBJS.glob("*.yaml")
        if path.read_text(encoding="utf-8").startswith("# B21_TFA_PLAN_GENERATED")
    ]

    assert generated
    assert any(
        any("Category" in field for field in yaml.safe_load(
            path.read_text(encoding="utf-8")
        ).get("fields", []))
        for path in generated
    )


def test_tales_workshop_categories_do_not_enable_legacy_plan_learning() -> None:
    tool = REPO_ROOT / "mods/B21_TalesFromAppalachia/tools/sync_workshop_plans.py"
    assert "PLAN_GATING_ENABLED = False" in tool.read_text(encoding="utf-8")
    found_workshop_category = False
    for path in TFA_COBJS.glob("*.yaml"):
        record = yaml.safe_load(path.read_text(encoding="utf-8"))
        fields = record.get("fields", [])
        if not any("Category" in field for field in fields):
            continue
        workbench = next(
            (
                field["WorkbenchKeyword"]["reference"]["object_id"]
                for field in fields
                if "WorkbenchKeyword" in field
            ),
            None,
        )
        found_workshop_category |= workbench in FO4_WORKSHOP_BENCHES
    assert found_workshop_category
