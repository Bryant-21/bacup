"""Tests for merge_dependency_graphs in bacup_lib.models."""
from __future__ import annotations

import pytest

from bacup_lib.models import (
    AssetRef,
    DependencyGraph,
    RecordNode,
    merge_dependency_graphs,
)


def _node(form_key: str, editor_id: str = "", record_type: str = "WEAP") -> RecordNode:
    return RecordNode(
        form_key=form_key,
        editor_id=editor_id or form_key,
        record_type=record_type,
    )


def _asset(asset_type: str, source_path: str) -> AssetRef:
    return AssetRef(asset_type=asset_type, source_path=source_path)


def _graph(root: RecordNode, records, assets, errors=None) -> DependencyGraph:
    return DependencyGraph(
        root=root,
        all_records=list(records),
        all_assets=list(assets),
        errors=list(errors or []),
    )


def test_merge_empty_raises():
    with pytest.raises(ValueError):
        merge_dependency_graphs([])


def test_merge_single_returns_same():
    g = _graph(_node("000800"), [_node("000800")], [_asset("nif", "a.nif")])
    merged = merge_dependency_graphs([g])
    assert merged is g


def test_merge_dedups_records_by_form_key():
    a_root = _node("000800", "WeapA")
    b_root = _node("000900", "WeapB")
    shared = _node("00FF00", "SharedAmmo", record_type="AMMO")

    g1 = _graph(a_root, [a_root, shared], [])
    g2 = _graph(b_root, [b_root, shared], [])

    merged = merge_dependency_graphs([g1, g2])

    form_keys = [r.form_key for r in merged.all_records]
    assert form_keys == ["000800", "00FF00", "000900"]
    assert merged.root is a_root


def test_merge_dedups_assets_by_type_and_path():
    root = _node("000800")
    g1 = _graph(
        root,
        [root],
        [_asset("nif", "Meshes/shared.nif"), _asset("texture", "Tex/a.dds")],
    )
    g2 = _graph(
        _node("000900"),
        [_node("000900")],
        [_asset("nif", "Meshes/shared.nif"), _asset("texture", "Tex/b.dds")],
    )

    merged = merge_dependency_graphs([g1, g2])

    assert [(a.asset_type, a.source_path) for a in merged.all_assets] == [
        ("nif", "Meshes/shared.nif"),
        ("texture", "Tex/a.dds"),
        ("texture", "Tex/b.dds"),
    ]


def test_merge_concatenates_errors():
    g1 = _graph(_node("000800"), [], [], errors=["e1"])
    g2 = _graph(_node("000900"), [], [], errors=["e2", "e3"])
    merged = merge_dependency_graphs([g1, g2])
    assert merged.errors == ["e1", "e2", "e3"]


def test_merge_root_is_first_graphs_root():
    a = _node("000800", "First")
    b = _node("000900", "Second")
    merged = merge_dependency_graphs([
        _graph(a, [a], []),
        _graph(b, [b], []),
    ])
    assert merged.root is a
