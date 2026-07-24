from pathlib import Path

from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex import decompile_pex


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "Fragments:TopicInfos:TIF_W05_MQ_001P_Wayward_Lace_0056A18B"
PEX_PATH = (
    REPO_ROOT
    / "mods"
    / "SeventySix"
    / "data"
    / "Scripts"
    / "fragments"
    / "topicinfos"
    / "tif_w05_mq_001p_wayward_lace_0056a18b.pex"
)
EXPECTED_MEMBER = """Function Fragment_End(ObjectReference akSpeakerRef)
    If WaywardMM
        WaywardMM.AddToMap(False)
    EndIf
EndFunction"""


def test_lacey_map_marker_patch_merges_as_the_exact_sole_member():
    patch = _script_patch_source(SCRIPT_NAME)

    assert patch is not None
    assert patch.strip() == EXPECTED_MEMBER
    skeleton = decompile_pex(PEX_PATH, fo4_api_compat=True)
    merged = _merge_script_method_patches(skeleton, patch)
    members = list(_iter_top_level_papyrus_members(merged.splitlines()))

    assert [name for _kind, name, _start, _end in members] == ["fragment_end"]
    _kind, _name, start, end = members[0]
    assert "\n".join(merged.splitlines()[start : end + 1]) == EXPECTED_MEMBER
    assert _merge_script_method_patches(merged, patch) == merged
