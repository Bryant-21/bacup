from __future__ import annotations

import csv
from pathlib import Path

import pytest

from bacup_lib.tests.test_terminal_fragment_script_patches import _fo4_base_source
from bacup_lib.workflows.unified import (
    _iter_top_level_papyrus_members,
    _merge_script_method_patches,
    _script_patch_source,
    _script_relative_path,
)
from creation_lib.pex import decompile_pex
from creation_lib.pex.native_runtime import compile_psc


REPO_ROOT = Path(__file__).resolve().parents[5]
DOCS = REPO_ROOT / "bacup" / "docs" / "stub_restoration"
DEPLOYED_ROOT = REPO_ROOT / "mods" / "SeventySix" / "data" / "Scripts"
GENERATED_ROOT = REPO_ROOT / "mods" / "SeventySix" / "Scripts" / "Source" / "User"
MARKER = "W05_MQ_004P_CacheLightMarkerScript"
QF = "Fragments:Quests:QF_W05_MQ_004P_Crane_0041C976"
CONTRACT = "contracts/w05-mq-004p-cache-light-chain.md"


def _member_body(source: str, member_name: str) -> str:
    lines = source.splitlines()
    start, end = next(
        (start, end)
        for kind, name, start, end in _iter_top_level_papyrus_members(lines)
        if kind in {"function", "event"} and name == member_name.lower()
    )
    return "\n".join(lines[start : end + 1])


def _merged(script_name: str) -> str:
    patch = _script_patch_source(script_name)
    assert patch is not None
    pex = DEPLOYED_ROOT / _script_relative_path(script_name, ".pex")
    return _merge_script_method_patches(
        decompile_pex(pex, fo4_api_compat=True), patch
    )


def test_cache_light_row_is_patched_and_stage_1000_starts_the_chain() -> None:
    with (DOCS / "status.csv").open(encoding="utf-8", newline="") as stream:
        row = next(
            row
            for row in csv.DictReader(stream)
            if row["relative_path"] == "W05_MQ_004P_CacheLightMarkerScript.psc"
        )
    assert row["terminal_state"] == "patched"
    assert row["evidence"] == CONTRACT

    qf = _script_patch_source(QF)
    assert qf is not None
    stage_1000 = _member_body(qf, "fragment_stage_1000_item_00")
    av = 'Game.GetFormFromFile(0x005911D2, "SeventySix.esm") as ActorValue'
    head = (
        'Game.GetFormFromFile(0x005911D1, "SeventySix.esm") as ObjectReference'
    )
    assert av in stage_1000
    assert head in stage_1000
    assert stage_1000.index("playerRef.SetValue(cacheOpenedValue, 1.0)") < (
        stage_1000.index("firstCacheLight.Enable()")
    )
    assert stage_1000.index("firstCacheLight.Enable()") < stage_1000.index(
        "cacheDoor.Unlock()"
    )


def test_marker_advances_only_one_still_disabled_link() -> None:
    marker = _script_patch_source(MARKER)
    assert marker is not None
    body = _member_body(marker, "onload")
    assert "playerRef.GetValue(TrackingValue) < 1.0" in body
    assert "nextMarker == None || !nextMarker.IsDisabled()" in body
    assert body.index("EnableSound.Play(Self)") < body.index(
        "Utility.Wait(EnableDelay)"
    )
    assert body.index("Utility.Wait(EnableDelay)") < body.index(
        "nextMarker.Enable()"
    )


@pytest.mark.parametrize("script_name", (MARKER, QF))
def test_cache_light_production_merge_is_idempotent_and_compiles(
    script_name: str,
) -> None:
    patch = _script_patch_source(script_name)
    assert patch is not None
    merged = _merged(script_name)
    assert _merge_script_method_patches(merged, patch) == merged

    base_source = _fo4_base_source()
    assert base_source is not None, "FO4 base Papyrus sources unavailable"
    result = compile_psc(
        merged,
        imports=[str(base_source), str(GENERATED_ROOT)],
        game="fo4",
        flags=str(base_source / "Institute_Papyrus_Flags.flg"),
        source_path=f"{script_name.rsplit(':', 1)[-1]}.psc",
    )
    diagnostics = "\n".join(str(item) for item in result.diagnostics)
    assert result.ok, diagnostics
    assert result.pex_bytes is not None
