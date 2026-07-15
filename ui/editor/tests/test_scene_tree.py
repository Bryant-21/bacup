from types import SimpleNamespace

from ui.editor.panels.scene_tree import (
    LARGE_TREE_DEFAULT_OPEN_BLOCK_LIMIT,
    _should_default_open_root,
)


def test_small_scene_tree_roots_default_open():
    nif = SimpleNamespace(blocks=[object()] * LARGE_TREE_DEFAULT_OPEN_BLOCK_LIMIT)

    assert _should_default_open_root(nif)


def test_large_scene_tree_roots_start_collapsed():
    nif = SimpleNamespace(blocks=[object()] * (LARGE_TREE_DEFAULT_OPEN_BLOCK_LIMIT + 1))

    assert not _should_default_open_root(nif)
