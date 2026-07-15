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


def test_identical_trees_no_problems():
    tree = {"data/x.esp": "aaa", "data/Meshes/a.nif": "bbb"}
    assert diff_trees(dict(tree), dict(tree)) == []


def test_nif_different_hash_same_path_ignored():
    g = {"data/Meshes/a.nif": "hash1"}
    a = {"data/Meshes/a.nif": "hash2"}
    assert diff_trees(g, a) == []


def test_nif_missing_in_actual_is_problem():
    g = {"data/Meshes/a.nif": "hash1"}
    a: dict[str, str] = {}
    problems = diff_trees(g, a)
    assert problems == ["data/Meshes/a.nif: missing in actual"]


def test_deterministic_type_hash_mismatch_is_problem():
    for name in ("data/x.dds", "data/x.esp", "data/x.yaml"):
        g = {name: "hash1"}
        a = {name: "hash2"}
        assert diff_trees(g, a) == [f"{name}: hash mismatch"]


def test_case_only_path_difference_no_problem():
    g = {"Meshes/Foo/Bar.dds": "h"}
    a = {"meshes/foo/bar.dds": "h"}
    assert diff_trees(g, a) == []


def test_ba2_bytes_differ_no_problem_but_missing_is_problem():
    g = {"data/Main.ba2": "h1"}
    assert diff_trees(g, {"data/Main.ba2": "h2"}) == []
    assert diff_trees(g, {}) == ["data/Main.ba2: missing in actual"]


def test_unexpected_in_actual_is_problem():
    g: dict[str, str] = {}
    a = {"data/x.dds": "h"}
    assert diff_trees(g, a) == ["data/x.dds: unexpected in actual"]


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
