from __future__ import annotations

from app.env_sync import parse_env_file, update_env_file


def test_update_env_file_preserves_comments_and_updates_keys(tmp_path):
    env_path = tmp_path / ".env"
    env_path.write_text(
        "# keep me\nDEFAULT_GAME=\"fo4\"\nFO4_DIR=\"old\"\n",
        encoding="utf-8",
    )

    update_env_file(
        {
            "DEFAULT_GAME": "skyrimse",
            "FO4_DIR": r"C:\Games\Fallout 4",
            "MOD_PREFIX": "B21",
        },
        env_path,
    )

    text = env_path.read_text(encoding="utf-8")
    assert "# keep me" in text
    assert 'DEFAULT_GAME="skyrimse"' in text
    assert 'FO4_DIR="C:\\Games\\Fallout 4"' in text
    assert 'MOD_PREFIX="B21"' in text
    assert parse_env_file(env_path)["DEFAULT_GAME"] == "skyrimse"


def test_update_env_file_creates_missing_file(tmp_path):
    env_path = tmp_path / ".env"

    update_env_file({"DEFAULT_GAME": "fo4"}, env_path)

    assert env_path.read_text(encoding="utf-8") == 'DEFAULT_GAME="fo4"\n'
