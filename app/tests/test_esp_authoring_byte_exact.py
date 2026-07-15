"""Byte-exact regression test for the native handle authoring export path.

Guards against changes in `serialize_record_payload_native`,
`compact_authoring_subrecord_impl`, and related helpers that would silently
alter the authoring-dir file tree. The fixture was generated from DLCworkshop02.esm
with the Phase A native exporter; if the comparison fails, regenerate per
plan section A.15.1 only if the divergence is intentional and approved.
"""

from __future__ import annotations

import filecmp
import os
import tarfile
from pathlib import Path

import pytest

from app.env_config import build_game_context_from_env
from app.env_sync import parse_env_file
from creation_lib.esp import Plugin, export_authoring_dir


FIXTURE_PATH = Path(__file__).parents[2] / "py_creation_lib" / "tests" / "fixtures" / "authoring-baseline-dlcworkshop02.tar.gz"
FIXTURE_ROOT_NAME = "baseline_dlcworkshop02_authoring"


def resolve_game_data_dir(game: str) -> Path | None:
    env = parse_env_file()
    env.update(os.environ)
    return build_game_context_from_env(game, env).data_dir


def _collect_diffs(cmp: filecmp.dircmp, out: list[str]) -> None:
    for name in cmp.left_only:
        out.append(f"only-in-baseline: {name}")
    for name in cmp.right_only:
        out.append(f"only-in-current: {name}")
    for name in cmp.diff_files:
        out.append(f"differs: {name}")
    for sub in cmp.subdirs.values():
        _collect_diffs(sub, out)


@pytest.mark.integration
def test_phaseA_authoring_dir_byte_exact_vs_baseline(tmp_path: Path) -> None:
    if not FIXTURE_PATH.is_file():
        pytest.skip(
            "baseline fixture missing — regenerate per plan step A.15.1 "
            f"({FIXTURE_PATH})"
        )
    data = resolve_game_data_dir("fo4")
    if data is None:
        pytest.skip("FO4_DATA not configured")
    plugin_path = data / "DLCworkshop02.esm"
    if not plugin_path.is_file():
        pytest.skip(f"DLCworkshop02.esm not found at {plugin_path}")

    baseline_root = tmp_path / "baseline"
    baseline_root.mkdir()
    with tarfile.open(FIXTURE_PATH, "r:gz") as tf:
        tf.extractall(baseline_root)
    baseline_dir = baseline_root / FIXTURE_ROOT_NAME

    current = tmp_path / "current_authoring"
    plugin = Plugin.load(plugin_path, game="fo4")
    export_authoring_dir(plugin, current, backend="native")

    comparison = filecmp.dircmp(str(baseline_dir), str(current))
    diffs: list[str] = []
    _collect_diffs(comparison, diffs)
    assert not diffs, f"authoring-dir byte-exact diverged: {diffs[:20]}"
