"""Tests for multi-game Papyrus LSP service."""
import pytest
from unittest.mock import patch
from pathlib import Path


class TestGameAwareLsp:
    def test_default_game_is_fo4(self):
        from ui.papyrus.papyrus_lsp_service import LspService
        svc = LspService()
        assert svc.game_id == "fo4"

    def test_set_game_to_skyrimse(self):
        from ui.papyrus.papyrus_lsp_service import LspService
        svc = LspService(game_id="skyrimse")
        assert svc.game_id == "skyrimse"
        assert "skyrimse_scripts.db" in svc._db_path

    def test_set_game_to_starfield(self):
        from ui.papyrus.papyrus_lsp_service import LspService
        svc = LspService(game_id="starfield")
        assert svc.game_id == "starfield"
        assert "starfield_scripts.db" in svc._db_path

    def test_fo76_has_no_papyrus(self):
        """FO76 should fall back to FO4 DB (no FO76 Papyrus)."""
        from ui.papyrus.papyrus_lsp_service import LspService
        svc = LspService(game_id="fo76")
        # Falls back to FO4 since FO76 has no Papyrus
        assert "fo4_scripts.db" in svc._db_path

    def test_discover_source_dirs_uses_game_env(self):
        """Source dir discovery should not crash when no extra dirs are configured."""
        from ui.papyrus.papyrus_lsp_service import LspService
        svc = LspService(game_id="skyrimse")
        dirs = svc._discover_source_dirs()
        assert isinstance(dirs, list)
