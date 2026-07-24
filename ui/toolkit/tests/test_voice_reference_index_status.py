from pathlib import Path
from types import SimpleNamespace

from creation_lib.audio import voice_reference
from creation_lib.ui import host
from creation_lib.ui.shell.settings_window import SettingsWindow
from ui.toolkit import setup_wizard


def test_setup_wizard_checks_voice_index_in_db_dir(monkeypatch, tmp_path: Path):
    game_root = tmp_path / "Fallout 4"
    (game_root / "Data").mkdir(parents=True)
    db_dir = tmp_path / "data"
    cache_path = db_dir / "fo4_voice_reference.db"
    cache_path.parent.mkdir()
    cache_path.touch()
    call_kwargs = {}

    def fake_cache_path(**kwargs):
        call_kwargs.update(kwargs)
        return cache_path

    monkeypatch.setattr(
        voice_reference,
        "voice_reference_sqlite_cache_path",
        fake_cache_path,
    )
    monkeypatch.setattr(setup_wizard, "get_db_dir", lambda: db_dir)

    wizard = object.__new__(setup_wizard.SetupWizard)
    wizard._game_paths = {"fo4": {"path": str(game_root)}}
    wizard._extracted_dirs = {}

    assert wizard._voice_index_exists("fo4") is True
    assert call_kwargs["db_dir"] == db_dir
    assert "cache_dir" not in call_kwargs


def test_settings_window_checks_voice_index_in_db_dir(monkeypatch, tmp_path: Path):
    game_root = tmp_path / "Fallout 4"
    (game_root / "Data").mkdir(parents=True)
    db_dir = tmp_path / "data"
    cache_path = db_dir / "fo4_voice_reference.db"
    cache_path.parent.mkdir()
    cache_path.touch()
    call_kwargs = {}

    def fake_cache_path(**kwargs):
        call_kwargs.update(kwargs)
        return cache_path

    monkeypatch.setattr(
        voice_reference,
        "voice_reference_sqlite_cache_path",
        fake_cache_path,
    )
    monkeypatch.setattr(
        host, "get_host", lambda: SimpleNamespace(get_db_dir=lambda: db_dir)
    )

    settings = SimpleNamespace(get_game_paths=lambda game: {})
    window = SettingsWindow(settings)
    window._game_paths = {"fo4": {"root": str(game_root)}}

    assert window._voice_reference_index_status("fo4") == (True, "0 MB")
    assert call_kwargs["db_dir"] == db_dir
    assert "cache_dir" not in call_kwargs
