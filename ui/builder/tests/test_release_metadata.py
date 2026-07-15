from __future__ import annotations

import json
from unittest.mock import mock_open, patch

from ui.builder.release_metadata import (
    latest_tracked_version,
    read_mod_version,
    render_changelog,
    render_release_notes,
    sanitize_release_token,
    update_release_history,
    write_mod_version,
)


def test_sanitize_release_token_normalizes_filename_text():
    assert sanitize_release_token("v1.2.3 beta / hotfix") == "v1.2.3_beta___hotfix"


def test_update_release_history_replaces_latest_matching_entry():
    release_dir = r"C:\fake\release"
    first = {
        "mod": "B21_Test",
        "version": "1.0.0",
        "released_at": "2026-04-12 11:00:00 EDT",
        "notes": "Initial release",
        "git_commit": "abc1234",
        "artifacts": ["B21_Test.esp"],
        "options": [],
    }
    second = dict(first)
    second["released_at"] = "2026-04-12 11:05:00 EDT"

    m = mock_open()
    with patch("ui.builder.release_metadata.read_release_history", side_effect=[[], [first]]), \
            patch("builtins.open", m), \
            patch("json.dump") as dump_mock, \
            patch("ui.builder.release_metadata.render_changelog", return_value="# Changelog\n"):
        history = update_release_history(str(release_dir), first)
        assert len(history) == 1
        history = update_release_history(str(release_dir), second)
        assert len(history) == 1
        assert history[0]["released_at"] == second["released_at"]
        assert dump_mock.call_count == 2


def test_latest_tracked_version_reads_most_recent_version():
    with patch("ui.builder.release_metadata.read_release_history", return_value=[
        {
            "mod": "B21_Test",
            "version": "2.1.0",
            "released_at": "2026-04-12 12:00:00 EDT",
            "notes": "",
            "git_commit": "",
            "artifacts": [],
            "options": [],
        }
    ]):
        assert latest_tracked_version(r"C:\fake\release") == "2.1.0"


def test_render_release_notes_includes_metadata_and_notes():
    text = render_release_notes({
        "mod": "B21_Test",
        "version": "1.2.0",
        "previous_version": "1.1.0",
        "released_at": "2026-04-12 12:00:00 EDT",
        "game": "fo4",
        "plugin": "B21_Test.esp",
        "git_commit": "deadbee",
        "options": ["Localized strings"],
        "artifacts": ["B21_Test.esp", "CHANGELOG.md"],
        "notes": "- Added new spawn points\n- Fixed archive paths",
    })
    assert "# B21_Test Release Notes" in text
    assert "- Version: 1.2.0" in text
    assert "- Previous Version: 1.1.0" in text
    assert "- Git Commit: deadbee" in text
    assert "- Localized strings" in text
    assert "- Added new spawn points" in text


def test_render_changelog_lists_versions_and_notes():
    text = render_changelog([
        {
            "version": "1.0.1",
            "previous_version": "1.0.0",
            "released_at": "2026-04-12 13:00:00 EDT",
            "notes": "- Fixed one issue",
            "git_commit": "1234abc",
        }
    ])
    assert "# Changelog" in text
    assert "## 1.0.1 (2026-04-12 13:00:00 EDT)" in text
    assert "- Changes since `1.0.0`" in text
    assert "- Fixed one issue" in text


def test_read_write_mod_version_round_trip():
    m = mock_open(read_data="1.0.1\n")
    with patch("ui.builder.release_metadata.os.path.isfile", return_value=True), \
            patch("builtins.open", m):
        assert read_mod_version(r"C:\fake\mod") == "1.0.1"

    m = mock_open()
    with patch("builtins.open", m):
        write_mod_version(r"C:\fake\mod", "1.0.2")
    handle = m()
    handle.write.assert_any_call("1.0.2")
