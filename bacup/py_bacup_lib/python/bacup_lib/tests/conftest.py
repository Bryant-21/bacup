"""Shared fixtures for conversion e2e tests."""
from __future__ import annotations

import pytest

from bacup_lib.tests.fo76_synthesizer import FO76Synthesizer


@pytest.fixture
def synthesizer_fo76() -> FO76Synthesizer:
    """FO76Synthesizer loaded with fo76_to_fo4 map."""
    s = FO76Synthesizer()
    s.load_map("fo76", "fo4")
    return s


@pytest.fixture
def default_game_context(tmp_path):
    """Minimal GameContext for tests that need to construct orchestrators.

    Uses tmp_path-based dirs so tests don't accidentally touch real game
    data. Tests that need a real extracted dir should override specific
    fields with pytest param.
    """
    from creation_lib.core.app_context import GameContext
    extracted = tmp_path / "extracted_fo4"
    data = tmp_path / "Fallout 4" / "Data"
    extracted.mkdir(parents=True, exist_ok=True)
    data.mkdir(parents=True, exist_ok=True)
    return GameContext(
        game="fo4",
        data_dir=data,
        extracted_dir=extracted,
        addon_index_start=20000,
    )
