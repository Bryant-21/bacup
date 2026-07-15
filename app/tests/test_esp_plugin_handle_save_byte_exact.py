"""Byte-exact regression test for the native handle save path.

A.10 shipped handle save as a materialize-and-delegate wrapper around
``save_plugin_native_impl`` (tracked by task #28). The existing
byte-exact roundtrip gate (``tests/test_esp_official_byte_exact.py``)
exercises ``Plugin.load``/``Plugin.save`` but does not touch the
handle load/save pair end-to-end. This test gives that path its own
tripwire.

Runs one small plugin per supported game. Skips at fixture level when the
game data dir is not configured or the target plugin isn't present.
"""
from __future__ import annotations

import hashlib
import os
from pathlib import Path

import pytest

from app.env_config import build_game_context_from_env
from app.env_sync import parse_env_file
from creation_lib.esp.native_runtime import plugin_handle_call, plugin_handle_load


GAME_PLUGINS = [
    ("fo4", "DLCRobot.esm"),
    ("skyrimse", "Update.esm"),
    ("fo76", "NW.esm"),
    ("starfield", "SFBGS006.esm"),
    ("fo3", "Zeta.esm"),
    ("fnv", "DeadMoney.esm"),
]


def resolve_game_data_dir(game: str) -> Path | None:
    env = parse_env_file()
    env.update(os.environ)
    return build_game_context_from_env(game, env).data_dir


@pytest.mark.integration
@pytest.mark.parametrize("game,plugin_name", GAME_PLUGINS)
def test_plugin_handle_save_byte_exact(
    tmp_path: Path, game: str, plugin_name: str
) -> None:
    data_dir = resolve_game_data_dir(game)
    if data_dir is None:
        pytest.skip(f"{game} data dir not configured")
    plugin_path = data_dir / plugin_name
    if not plugin_path.is_file():
        pytest.skip(f"{plugin_name} not found at {plugin_path}")

    original_bytes = plugin_path.read_bytes()
    handle = plugin_handle_load(str(plugin_path), game=game)
    out = tmp_path / plugin_name
    plugin_handle_call(handle, "save", str(out))
    saved_bytes = out.read_bytes()

    if saved_bytes != original_bytes:
        orig_hash = hashlib.sha256(original_bytes).hexdigest()[:16]
        saved_hash = hashlib.sha256(saved_bytes).hexdigest()[:16]
        pytest.fail(
            f"{game}:{plugin_name} byte-exact mismatch "
            f"(orig={len(original_bytes)}B sha={orig_hash} "
            f"saved={len(saved_bytes)}B sha={saved_hash})"
        )
