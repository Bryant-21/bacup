from __future__ import annotations

import pytest

from bacup_lib.pipboy_quest_catalog import OUTPUT, emit_quest_catalog


def test_missing_source_movie_removes_a_stale_catalog_before_fallback(tmp_path) -> None:
    destination = tmp_path / OUTPUT
    destination.parent.mkdir(parents=True)
    destination.write_text("stale", encoding="utf-8")
    with pytest.raises(FileNotFoundError):
        emit_quest_catalog(tmp_path / "SeventySix.esm", tmp_path / "converted.esm",
                           tmp_path / "source", tmp_path)
    assert not destination.exists()
