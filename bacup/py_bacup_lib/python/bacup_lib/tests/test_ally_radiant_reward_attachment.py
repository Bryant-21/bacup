"""Validate the Papyrus script bound by native ally-radiant reward attachment."""

from __future__ import annotations

import os
import subprocess
from pathlib import Path

import pytest

# --- the script the attachment binds to ------------------------------------
#
# The native compiler accepts unknown identifiers and member calls, so each
# property encoded here is also resolved through the Papyrus LSP, and the script
# is compiled with stock PapyrusCompiler.exe.

from creation_lib.papyrus_lsp import ScriptDB

REWARD_SCRIPT = "B21:QuestRewards"
ADDITION_SOURCE = (
    Path(__file__).resolve().parents[1]
    / "script_additions"
    / "fo76_fo4"
    / "B21"
    / "QuestRewards.psc"
)
BOUND_PROPERTIES = (
    "CapsStages",
    "RewardCaps",
    "CapsItem",
    "ItemStages",
    "RewardItems",
    "RewardCounts",
)


def _fo4_root() -> Path | None:
    # Read the repo `.env` when the variable is not exported, so this check runs
    # instead of silently skipping — a skipped compile is not evidence.
    raw = os.environ.get("FO4_DIR", "").strip().strip('"')
    if not raw:
        env_file = Path(__file__).resolve().parents[5] / ".env"
        if env_file.is_file():
            for line in env_file.read_text(encoding="utf-8").splitlines():
                key, _, value = line.partition("=")
                if key.strip() == "FO4_DIR":
                    raw = value.strip().strip('"')
                    break
    if not raw:
        return None
    root = Path(raw)
    if not (root / "Papyrus Compiler" / "PapyrusCompiler.exe").is_file():
        return None
    if not (root / "Data" / "Scripts" / "Source" / "Base").is_dir():
        return None
    return root


def test_every_bound_property_resolves_through_the_papyrus_lsp(tmp_path) -> None:
    source_root = tmp_path / "src" / "B21"
    source_root.mkdir(parents=True)
    (source_root / "QuestRewards.psc").write_text(
        ADDITION_SOURCE.read_text(encoding="utf-8"), encoding="utf-8"
    )
    db = ScriptDB(str(tmp_path / "lsp.db"), source_dirs=[str(tmp_path / "src")])
    try:
        assert db.script_exists(REWARD_SCRIPT)
        for name in BOUND_PROPERTIES:
            assert db.has_property(REWARD_SCRIPT, name), name
        assert db.has_event(REWARD_SCRIPT, "OnStageSet")
        assert db.has_event(REWARD_SCRIPT, "OnQuestInit")
        assert db.has_function(REWARD_SCRIPT, "GrantRewardsForStage")
        # The reward must be paid once per quest instance.
        assert db.has_function(REWARD_SCRIPT, "HasGrantedStage")
        assert db.has_function(REWARD_SCRIPT, "RecordGrantedStage")
        # Negative control: the resolver is not accepting any name at all.
        assert not db.has_property(REWARD_SCRIPT, "CapsStagesZ")
    finally:
        db.close()


@pytest.mark.skipif(
    _fo4_root() is None, reason="FO4 PapyrusCompiler.exe / Base scripts unavailable"
)
def test_the_reward_script_compiles_under_stock_papyruscompiler(tmp_path) -> None:
    root = _fo4_root()
    assert root is not None
    user_root = tmp_path / "User"
    (user_root / "B21").mkdir(parents=True)
    (user_root / "B21" / "QuestRewards.psc").write_text(
        ADDITION_SOURCE.read_text(encoding="utf-8"), encoding="utf-8"
    )
    base = root / "Data" / "Scripts" / "Source" / "Base"
    out = tmp_path / "out"
    out.mkdir()
    result = subprocess.run(
        [
            str(root / "Papyrus Compiler" / "PapyrusCompiler.exe"),
            "B21/QuestRewards",
            f"-import={user_root};{base}",
            f"-output={out}",
            "-f=Institute_Papyrus_Flags.flg",
        ],
        cwd=str(user_root),
        capture_output=True,
        text=True,
    )
    assert (out / "B21" / "QuestRewards.pex").is_file(), result.stdout + result.stderr
