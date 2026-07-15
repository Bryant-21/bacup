"""Tests for NifFileBrowserPanel helpers (no ImGui required)."""
import os
import pytest
from unittest.mock import MagicMock


def _make_panel(sessions=None):
    """Panel instance with minimal mock app."""
    from ui.editor.panels.nif_file_browser import NifFileBrowserPanel
    app = MagicMock()
    app.registry.all_sessions.return_value = list(sessions or [])
    app.registry.get_session.return_value = None
    return NifFileBrowserPanel(app)


class TestScanDir:
    def test_returns_only_nif_like_files(self, tmp_path):
        """_scan_dir returns NIF-like files and subdirs; ignores other files."""
        (tmp_path / "weapon.nif").write_text("")
        (tmp_path / "terrain.bto").write_text("")
        (tmp_path / "landscape.btr").write_text("")
        (tmp_path / "texture.dds").write_text("")
        (tmp_path / "readme.txt").write_text("")

        panel = _make_panel()
        entries = panel._scan_dir(str(tmp_path))
        names = [name for _, name, _ in entries]
        assert "weapon.nif" in names
        assert "terrain.bto" in names
        assert "landscape.btr" in names
        assert "texture.dds" not in names
        assert "readme.txt" not in names

    def test_includes_subdirectories(self, tmp_path):
        """_scan_dir includes subdirectory entries."""
        subdir = tmp_path / "parts"
        subdir.mkdir()
        (tmp_path / "weapon.nif").write_text("")

        panel = _make_panel()
        entries = panel._scan_dir(str(tmp_path))
        names = [name for _, name, _ in entries]
        assert "parts" in names
        assert "weapon.nif" in names

    def test_dirs_sort_before_files(self, tmp_path):
        """Directories appear before .nif files in results."""
        (tmp_path / "aaa.nif").write_text("")
        (tmp_path / "zzz").mkdir()

        panel = _make_panel()
        entries = panel._scan_dir(str(tmp_path))
        is_dirs = [is_dir for is_dir, _, _ in entries]
        # All True entries should come before all False entries
        saw_file = False
        for is_dir in is_dirs:
            if not is_dir:
                saw_file = True
            elif saw_file:
                pytest.fail("Dir appeared after file in sorted results")

    def test_alphabetical_within_group(self, tmp_path):
        """Files within the same group are sorted alphabetically (case-insensitive)."""
        (tmp_path / "Zebra.nif").write_text("")
        (tmp_path / "apple.nif").write_text("")
        (tmp_path / "Mango.nif").write_text("")

        panel = _make_panel()
        entries = panel._scan_dir(str(tmp_path))
        file_names = [name for is_dir, name, _ in entries if not is_dir]
        assert file_names == sorted(file_names, key=str.lower)

    def test_nif_like_extensions_case_insensitive(self, tmp_path):
        """Files with uppercase NIF-like extensions are included."""
        (tmp_path / "weapon.NIF").write_text("")
        (tmp_path / "scope.Nif").write_text("")
        (tmp_path / "tree.BTO").write_text("")
        (tmp_path / "terrain.Btr").write_text("")

        panel = _make_panel()
        entries = panel._scan_dir(str(tmp_path))
        names = [name for _, name, _ in entries]
        assert "weapon.NIF" in names
        assert "scope.Nif" in names
        assert "tree.BTO" in names
        assert "terrain.Btr" in names

    def test_oserror_returns_empty(self, tmp_path):
        """_scan_dir returns empty list on permission/OS error."""
        panel = _make_panel()
        entries = panel._scan_dir(str(tmp_path / "nonexistent"))
        assert entries == []


class TestIsAttached:
    def test_returns_true_when_session_path_matches(self, tmp_path):
        """_is_attached returns True when a session has the same path."""
        nif_path = str(tmp_path / "weapon.nif")
        session = MagicMock()
        session.file_path = nif_path

        panel = _make_panel(sessions=[session])
        assert panel._is_attached(nif_path) is True

    def test_returns_false_when_no_match(self, tmp_path):
        """_is_attached returns False when no session matches."""
        session = MagicMock()
        session.file_path = str(tmp_path / "other.nif")

        panel = _make_panel(sessions=[session])
        assert panel._is_attached(str(tmp_path / "weapon.nif")) is False

    def test_path_comparison_normalizes_separators(self, tmp_path):
        """_is_attached matches despite mixed path separators."""
        forward = str(tmp_path / "weapon.nif").replace("\\", "/")
        backward = str(tmp_path / "weapon.nif").replace("/", "\\")

        session = MagicMock()
        session.file_path = forward

        panel = _make_panel(sessions=[session])
        # Should match regardless of separator style
        assert panel._is_attached(backward) is True


class TestWindowName:
    def test_window_name_set_in_init(self):
        """window_name is set in __init__ for workspace renaming."""
        panel = _make_panel()
        assert panel.window_name == "File Browser"
