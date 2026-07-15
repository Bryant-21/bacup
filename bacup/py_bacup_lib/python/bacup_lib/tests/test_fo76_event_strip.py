"""Tests for FO76 → FO4 animation event stripping."""
from __future__ import annotations

from bacup_lib.orchestrator import _strip_source_game_events_from_hkx


def test_strip_returns_zeros_for_missing_mapping(tmp_path):
    """Unknown source game → no mapping YAML → function is a no-op."""
    fake = tmp_path / "fake.hkx"
    fake.write_bytes(b"not even an hkx")
    dropped, renamed, warnings = _strip_source_game_events_from_hkx(
        str(fake), "skyrimse", "fo4"
    )
    assert dropped == 0
    assert renamed == 0
    assert warnings == []


def test_strip_returns_zeros_for_unreadable_file(tmp_path):
    """Non-HKX bytes → reader throws → function returns zeros gracefully."""
    fake = tmp_path / "fake.hkx"
    fake.write_bytes(b"not an hkx file")
    dropped, renamed, _warnings = _strip_source_game_events_from_hkx(
        str(fake), "fo76", "fo4"
    )
    assert dropped == 0
    assert renamed == 0
