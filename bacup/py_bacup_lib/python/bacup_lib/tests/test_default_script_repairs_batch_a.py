from __future__ import annotations

import re
from pathlib import Path

from bacup_lib.workflows.unified import _merge_script_method_patches, _script_patch_source
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SCRIPT_NAME = "DefaultTopicInfoTriggerCombat"
SOURCE_PEX = (
    REPO_ROOT
    / "extracted"
    / "fo76"
    / "scripts"
    / "client"
    / f"{SCRIPT_NAME}.pex"
)
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
DEPLOYED_PEX_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"


def _merged_source() -> str:
    skeleton = decompile_pex(SOURCE_PEX, fo4_api_compat=True)
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def test_default_topic_info_trigger_combat_merges_one_timed_handler_per_event():
    patch = _script_patch_source(SCRIPT_NAME)
    assert patch is not None
    assert "Scriptname " not in patch

    merged = _merged_source()
    assert merged.lower().count("event onbegin(objectreference akspeakerref, bool abhasbeensaid)") == 1
    assert merged.lower().count("event onend(objectreference akspeakerref, bool abhasbeensaid)") == 1

    begin = re.search(
        r"Event OnBegin\(ObjectReference akSpeakerRef, Bool abHasBeenSaid\)(.*?)EndEvent",
        merged,
        re.DOTALL,
    )
    end = re.search(
        r"Event OnEnd\(ObjectReference akSpeakerRef, Bool abHasBeenSaid\)(.*?)EndEvent",
        merged,
        re.DOTALL,
    )
    assert begin is not None
    assert end is not None
    assert "If !TriggerOnEnd" in begin.group(1)
    assert "If TriggerOnEnd" in end.group(1)
    for handler in (begin.group(1), end.group(1)):
        assert "Actor speaker = akSpeakerRef as Actor" in handler
        assert "If speaker != None" in handler
        assert handler.index("If TargetInitiatesCombat") < handler.index(
            "Game.GetPlayer().StartCombat(speaker)"
        ) < handler.index("Else") < handler.index(
            "speaker.StartCombat(Game.GetPlayer())"
        )


def test_default_topic_info_trigger_combat_merged_source_native_compiles():
    result = compile_psc(
        _merged_source(),
        imports=[str(SOURCE_ROOT), str(DEPLOYED_PEX_ROOT)],
        game="fo4",
        source_path=f"{SCRIPT_NAME}.psc",
    )

    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
