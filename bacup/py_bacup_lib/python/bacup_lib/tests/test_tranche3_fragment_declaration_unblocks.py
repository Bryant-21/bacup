from __future__ import annotations

import pytest

from bacup_lib.workflows.unified import _augment_fo76_to_fo4_script_skeleton


@pytest.mark.parametrize(
    ("script_name", "existing_property"),
    (
        (
            "Fragments:TopicInfos:TIF_V94_3_Personal_003EE27B",
            "V94_3_Pump_RobotBetaHasRespawnedValue",
        ),
        (
            "fragments:topicinfos:tif_v94_3_personal_003ee27e",
            "V94_3_Pump_RobotAlphaHasRespawnedValue",
        ),
    ),
)
def test_v94_topic_fragment_restores_record_bound_actor_value_declaration(
    script_name: str, existing_property: str
) -> None:
    skeleton = (
        f"Scriptname {script_name} Extends TopicInfo hidden\n\n"
        f"ActorValue Property {existing_property} Auto Mandatory\n"
    )

    augmented = _augment_fo76_to_fo4_script_skeleton(script_name, skeleton)

    declaration = (
        "ActorValue Property V94_3_Pump_RobotHasPlayedSpawnLineValue Auto Mandatory"
    )
    assert augmented.count(declaration) == 1
    assert augmented.count(existing_property) == 1
    assert _augment_fo76_to_fo4_script_skeleton(script_name, augmented) == augmented


def test_v94_topic_fragment_rejects_conflicting_record_bound_declaration() -> None:
    script_name = "Fragments:TopicInfos:TIF_V94_3_Personal_003EE27B"
    skeleton = (
        f"Scriptname {script_name} Extends TopicInfo hidden\n\n"
        "Int Property V94_3_Pump_RobotHasPlayedSpawnLineValue Auto Mandatory\n"
    )

    with pytest.raises(ValueError, match="conflicting Papyrus property"):
        _augment_fo76_to_fo4_script_skeleton(script_name, skeleton)
