"""Tests for creation_lib.app_paths.load_dotenv_into_environ."""
from __future__ import annotations

import os

import pytest

from app.paths import load_dotenv_into_environ


def test_load_dotenv_basic_keys(tmp_path, monkeypatch):
    """Parses KEY=value pairs and writes them into os.environ."""
    env_path = tmp_path / ".env"
    env_path.write_text(
        "FO4_DIR=C:/Games/Fallout4\n"
        "FO4_EXTRACTED_DIR=C:/extracted/fo4\n",
        encoding="utf-8",
    )
    monkeypatch.delenv("FO4_DIR", raising=False)
    monkeypatch.delenv("FO4_EXTRACTED_DIR", raising=False)

    result = load_dotenv_into_environ(env_path)
    assert result == env_path
    assert os.environ["FO4_DIR"] == "C:/Games/Fallout4"
    assert os.environ["FO4_EXTRACTED_DIR"] == "C:/extracted/fo4"


def test_load_dotenv_does_not_override_existing(tmp_path, monkeypatch):
    """Pre-existing env vars are preserved unless override=True."""
    env_path = tmp_path / ".env"
    env_path.write_text("FO4_DIR=from_env_file\n", encoding="utf-8")
    monkeypatch.setenv("FO4_DIR", "from_shell")

    load_dotenv_into_environ(env_path)
    assert os.environ["FO4_DIR"] == "from_shell"


def test_load_dotenv_override_true_replaces(tmp_path, monkeypatch):
    """With override=True the env-file value wins."""
    env_path = tmp_path / ".env"
    env_path.write_text("FO4_DIR=from_env_file\n", encoding="utf-8")
    monkeypatch.setenv("FO4_DIR", "from_shell")

    load_dotenv_into_environ(env_path, override=True)
    assert os.environ["FO4_DIR"] == "from_env_file"


def test_load_dotenv_skips_comments_and_blanks(tmp_path, monkeypatch):
    """Comment lines and blank lines do not write into the environment."""
    env_path = tmp_path / ".env"
    env_path.write_text(
        "# This is a comment\n"
        "\n"
        "FO4_DIR=set\n"
        "   # indented comment\n"
        "no_equals_sign\n",
        encoding="utf-8",
    )
    monkeypatch.delenv("FO4_DIR", raising=False)

    load_dotenv_into_environ(env_path)
    assert os.environ.get("FO4_DIR") == "set"


def test_load_dotenv_strips_quoted_values(tmp_path, monkeypatch):
    """Surrounding double or single quotes are removed."""
    env_path = tmp_path / ".env"
    env_path.write_text(
        'FO4_DIR="C:/Path With Spaces/Fallout4"\n'
        "FO4_EXTRACTED_DIR='C:/Single Quoted/extracted'\n",
        encoding="utf-8",
    )
    monkeypatch.delenv("FO4_DIR", raising=False)
    monkeypatch.delenv("FO4_EXTRACTED_DIR", raising=False)

    load_dotenv_into_environ(env_path)
    assert os.environ["FO4_DIR"] == "C:/Path With Spaces/Fallout4"
    assert os.environ["FO4_EXTRACTED_DIR"] == "C:/Single Quoted/extracted"


def test_load_dotenv_missing_file_returns_none(tmp_path):
    """A non-existent .env returns None and does not raise."""
    env_path = tmp_path / "does_not_exist" / ".env"
    assert load_dotenv_into_environ(env_path) is None


def test_load_dotenv_expands_var_references(tmp_path, monkeypatch):
    """${VAR} and $VAR references against already-set values are expanded."""
    env_path = tmp_path / ".env"
    env_path.write_text(
        "BASE=/games\n"
        "FO4_DIR=${BASE}/fallout4\n",
        encoding="utf-8",
    )
    monkeypatch.delenv("BASE", raising=False)
    monkeypatch.delenv("FO4_DIR", raising=False)

    load_dotenv_into_environ(env_path)
    assert os.environ["BASE"] == "/games"
    assert os.environ["FO4_DIR"] == "/games/fallout4"
