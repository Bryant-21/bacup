"""Tests for texture-set editing behavior."""

from unittest.mock import MagicMock

from ui.editor.panels.texture_editor import TextureEditorPanel


class _FakeBlock:
    def __init__(self, block_id: int, fields: dict):
        self.block_id = block_id
        self._fields = dict(fields)

    def get_field(self, name: str):
        return self._fields.get(name)

    def set_field(self, name: str, value):
        self._fields[name] = value


class _FakeNif:
    def __init__(self, block: _FakeBlock):
        self._block = block

    def get_block(self, block_id: int):
        return self._block if block_id == self._block.block_id else None


class _FakeRegistry:
    active_id = "main"


class _FakeApp:
    def __init__(self, block: _FakeBlock):
        self.nif_file = _FakeNif(block)
        self.undo_manager = MagicMock()
        self.registry = _FakeRegistry()
        self._nif_dirty = False


def test_set_texture_skips_repeated_semantic_noop():
    block = _FakeBlock(
        4,
        {"Textures[7]": {"Length": 8, "Value": list("old.dds")}},
    )
    app = _FakeApp(block)
    panel = TextureEditorPanel(app)
    cleared = {"Length": 0, "Value": []}

    panel._set_texture(block, "Textures[7]", block.get_field("Textures[7]"), cleared)

    assert block.get_field("Textures[7]") == cleared
    assert app.undo_manager.push.call_count == 1
    assert app._nif_dirty is True

    panel._set_texture(block, "Textures[7]", block.get_field("Textures[7]"), cleared)

    assert app.undo_manager.push.call_count == 1


def test_set_texture_skips_semantically_equal_texture_list():
    textures = [
        {"Length": 8, "Value": list("a.dds")},
        {"Length": 0, "Value": []},
    ]
    block = _FakeBlock(4, {"Textures": textures})
    app = _FakeApp(block)
    panel = TextureEditorPanel(app)

    panel._set_texture(
        block,
        "Textures",
        block.get_field("Textures"),
        [
            {"Length": 8, "Value": list("a.dds")},
            {"Length": 0, "Value": []},
        ],
    )

    assert app.undo_manager.push.call_count == 0
