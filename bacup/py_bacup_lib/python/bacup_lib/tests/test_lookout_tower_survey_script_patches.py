from __future__ import annotations

import re
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _merge_script_method_patches,
    _script_patch_source,
)
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
SOURCE_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"


def _merged() -> str:
    skeleton = (SOURCE_ROOT / "LookoutTowerQuestScript.psc").read_text(encoding="utf-8")
    patch = _script_patch_source("LookoutTowerQuestScript")
    assert patch is not None
    return _merge_script_method_patches(skeleton, patch)


def test_survey_reveals_markers_by_radius_not_by_alias():
    merged = _merged()
    assert 'Game.GetFormFromFile(aiMarkerID, "SeventySix.esm")' in merged
    assert "marker.GetDistance(akSurveyor) > 40000.0" in merged
    # AddToMap(False) reveals the marker without granting fast travel.
    assert "marker.AddToMap(False)" in merged
    assert "LookoutTowerSurveyMessage.Show(revealedCount)" in merged


def test_survey_drops_the_alias_fill_that_fo4_cannot_express():
    merged = _merged()
    # MapMarkersToReveal's only FO76 fill rule was ALFF, which conversion drops
    # because FO4's ALRT is legal only paired with ALFA against a Location
    # alias. The collection can never fill, so it must not gate the reveal.
    assert "MapMarkersToReveal.GetCount()" not in merged
    assert len(re.findall(r"^Event\s+OnQuestInit", merged, re.M)) == 1


def test_survey_candidates_cover_a_known_tower():
    merged = _merged()
    # Camp Adams Lookout (tower 10) reveals these; both resolve through
    # LCTN -> MasterSpecialReferences[LocRefType=MapMarkerRefType].
    assert "0x0005D32C" in merged  # Billings Homestead
    assert "0x002C9329" in merged  # Charleston Station


def test_survey_patch_compiles():
    base_source = _fo4_base_source()
    if base_source is None:
        pytest.skip("FO4 base Papyrus sources unavailable")
    result = compile_psc(
        _merged(),
        imports=[str(SOURCE_ROOT), str(base_source)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path="LookoutTowerQuestScript.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
