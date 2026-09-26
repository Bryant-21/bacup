# bacup/tests/conversion/test_golden_harness.py
"""Unit tests for the determinism-aware golden harness logic.

Pure dict-level checks of diff_trees / _is_excluded / _is_set_only — no
filesystem, no conversion. The harness must byte-compare deterministic
artifacts (esp/yaml/dds/...) but enforce only case-insensitive presence +
count parity for the non-deterministic classes (.nif/.ba2, rayon-parallel).
"""
from __future__ import annotations

import importlib.util
import sys
from pathlib import Path

import pytest

_REPO = Path(__file__).resolve().parents[3]


def _load_golden_module():
    path = _REPO / "bacup" / "scripts" / "conversion_golden.py"
    spec = importlib.util.spec_from_file_location("conversion_golden", path)
    module = importlib.util.module_from_spec(spec)
    sys.modules["conversion_golden"] = module
    spec.loader.exec_module(module)
    return module


golden = _load_golden_module()
diff_trees = golden.diff_trees
_is_excluded = golden._is_excluded
_is_set_only = golden._is_set_only


@pytest.mark.parametrize(
    "golden_tree,actual_tree,expected",
    [
        ({"data/x.esp": "aaa", "data/Meshes/a.nif": "bbb"},
         {"data/x.esp": "aaa", "data/Meshes/a.nif": "bbb"}, []),
        ({"data/Meshes/a.nif": "hash1"}, {"data/Meshes/a.nif": "hash2"}, []),
        ({"data/Main.ba2": "h1"}, {"data/Main.ba2": "h2"}, []),
        ({"Meshes/Foo/Bar.dds": "h"}, {"meshes/foo/bar.dds": "h"}, []),
        ({"data/Meshes/a.nif": "hash1"}, {}, ["data/Meshes/a.nif: missing in actual"]),
        ({"data/Main.ba2": "h1"}, {}, ["data/Main.ba2: missing in actual"]),
        ({"data/x.dds": "hash1"}, {"data/x.dds": "hash2"}, ["data/x.dds: hash mismatch"]),
        ({"data/x.esp": "hash1"}, {"data/x.esp": "hash2"}, ["data/x.esp: hash mismatch"]),
        ({"data/x.yaml": "hash1"}, {"data/x.yaml": "hash2"}, ["data/x.yaml: hash mismatch"]),
        ({}, {"data/x.dds": "h"}, ["data/x.dds: unexpected in actual"]),
    ],
)
def test_diff_trees(golden_tree, actual_tree, expected):
    assert diff_trees(golden_tree, actual_tree) == expected


def test_is_set_only():
    assert _is_set_only("data/Meshes/a.nif")
    assert _is_set_only("data/Main.BA2")  # case-insensitive
    assert not _is_set_only("data/x.dds")
    assert not _is_set_only("data/x.esp")


def test_is_excluded():
    # diagnostic debug dirs (beside data/, never inside it) are dropped
    assert _is_excluded("debug/x.json")
    assert _is_excluded("debug/terrain/terrain_timing.json")
    assert _is_excluded("SeventySix/debug/terrain/terrain_timing.json")
    assert _is_excluded("foo.log")
    assert _is_excluded("foo.LOG")
    # _NONDETERMINISTIC member with different case
    assert _is_excluded("data/CONVERSION_TIMING.JSON")

    # real assets under data/.../debug/ must NOT be excluded
    assert not _is_excluded("SeventySix/data/materials/effects/debug/debug_blue.bgem")
    assert not _is_excluded("data/Materials/foo/debug/bar.bgsm")
    assert not _is_excluded("data/Meshes/x.nif")
    assert not _is_excluded("SeventySix/x.esm")
