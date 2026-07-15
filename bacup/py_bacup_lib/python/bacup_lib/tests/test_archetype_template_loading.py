from __future__ import annotations

from pathlib import Path

import pytest

from bacup_lib.behavior.templates._schema import (
    available_template_archetypes,
    load_behavior_template,
    load_behavior_template_file,
)


EXPECTED_ARCHETYPES = {
    "deathclaw",
    "quadruped_mammal",
    "mirelurk",
    "scorpion_8leg",
    "ghoul_humanoid",
    "robot_handy",
    "robot_securitron",
    "insect_winged",
    "generic_quadruped",
}


def test_all_owned_archetype_templates_are_discoverable() -> None:
    assert EXPECTED_ARCHETYPES.issubset(set(available_template_archetypes()))


def test_deathclaw_template_has_expected_output_names() -> None:
    template = load_behavior_template("deathclaw")

    assert template.archetype == "deathclaw"
    assert template.behavior_name == "DeathclawEverything"
    assert template.root_behavior_name == "DeathclawRootBehavior"
    assert template.project_name == "DeathclawProject"
    assert template.character_name == "Deathclaw"
    assert template.model_after == "deathclaw"
    assert [state.role for state in template.states[:3]] == [
        "idle",
        "locomotion",
        "attack",
    ]
    death_state = next(state for state in template.states if state.role == "death")
    assert death_state.required is False


def test_each_bundled_template_has_distinct_model_after_metadata() -> None:
    model_after = {
        archetype: load_behavior_template(archetype).model_after
        for archetype in EXPECTED_ARCHETYPES
    }

    assert len(set(model_after.values())) == len(model_after)
    assert model_after["generic_quadruped"] == "generic_quadruped"


def test_invalid_template_missing_states_raises(tmp_path: Path) -> None:
    bad_template = tmp_path / "bad.yaml"
    bad_template.write_text(
        "\n".join(
            [
                "archetype: bad_case",
                "behavior_name: BadEverything",
                "root_behavior_name: BadRootBehavior",
                "project_name: BadProject",
                "character_name: Bad",
                "model_after: bad_case",
                "transitions: []",
            ]
        ),
        encoding="utf-8",
    )

    with pytest.raises(ValueError, match="states"):
        load_behavior_template_file(bad_template)
