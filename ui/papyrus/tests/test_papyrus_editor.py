# ui/editor/tests/test_papyrus_editor.py
import os
import pytest
from unittest.mock import MagicMock, patch


def test_editor_buffer_creation(tmp_path):
    """PapyrusEditorApp.open_file creates an EditorBuffer."""
    from ui.papyrus.papyrus_editor import PapyrusEditorApp

    psc_file = tmp_path / "TestScript.psc"
    psc_file.write_text("ScriptName TestScript\n")

    mock_lsp = MagicMock()
    app = PapyrusEditorApp(lsp=mock_lsp)
    app.open_file(str(psc_file))

    assert str(psc_file) in app.open_files
    buf = app.open_files[str(psc_file)]
    assert buf.text == "ScriptName TestScript\n"
    assert buf.dirty is False
    assert app.active_path == str(psc_file)

    # Cleanup
    app.close_file(str(psc_file))


def test_close_file_removes_buffer(tmp_path):
    """close_file removes buffer and clears active_path."""
    from ui.papyrus.papyrus_editor import PapyrusEditorApp

    psc_file = tmp_path / "Test.psc"
    psc_file.write_text("ScriptName Test\n")

    mock_lsp = MagicMock()
    app = PapyrusEditorApp(lsp=mock_lsp)
    app.open_file(str(psc_file))
    app.close_file(str(psc_file))

    assert str(psc_file) not in app.open_files
    assert app.active_path is None
