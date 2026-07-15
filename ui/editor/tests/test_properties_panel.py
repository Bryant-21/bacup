"""Tests for properties panel drag finalization."""

from unittest.mock import MagicMock

from creation_lib.nif.actions import SetFieldAction
from creation_lib.nif.nif_file import NifFile
from ui.editor.panels.properties import PropertiesPanel


class _FakeBlock:
    def __init__(self, block_id: int, value: float):
        self.block_id = block_id
        self._value = value

    def get_field(self, name: str):
        assert name == "Grayscale to Palette Scale"
        return self._value


class _FakeRegistry:
    def __init__(self):
        self.active_id = "main"


class _FakeApp:
    def __init__(self):
        self.registry = _FakeRegistry()
        self.undo_manager = MagicMock()
        self.rebuild_scene_from_nif = MagicMock()


def test_finalize_drag_reapplies_palette_scale_after_rebuild():
    app = _FakeApp()
    panel = PropertiesPanel(app)
    block = _FakeBlock(block_id=53, value=0.75)
    events: list[str] = []

    app.undo_manager.push.side_effect = lambda *args, **kwargs: events.append("undo")
    app.rebuild_scene_from_nif.side_effect = lambda *args, **kwargs: events.append("rebuild")
    panel._push_palette_scale = MagicMock(
        side_effect=lambda *args, **kwargs: events.append("reapply")
    )

    panel._drag_field = "53:Grayscale to Palette Scale"
    panel._drag_old_value = 0.25

    panel._finalize_drag(block, "Grayscale to Palette Scale", current_value=0.10)

    assert events == ["undo", "rebuild", "reapply"]
    app.undo_manager.push.assert_called_once()
    pushed_action = app.undo_manager.push.call_args.args[1]
    assert isinstance(pushed_action, SetFieldAction)
    assert pushed_action.old_value == 0.25
    assert pushed_action.new_value == 0.75
    panel._push_palette_scale.assert_called_once_with(block, 0.75)
    assert panel._drag_field is None
    assert panel._drag_old_value is None


def test_set_field_skips_noop_edits():
    app = _FakeApp()
    panel = PropertiesPanel(app)
    block = _FakeBlock(block_id=53, value=0.75)

    panel._set_field(block, "Grayscale to Palette Scale", 0.75, 0.75)

    app.undo_manager.push.assert_not_called()
    app.rebuild_scene_from_nif.assert_not_called()


def test_reusable_block_field_renderer_passes_schema_defs_to_widgets():
    app = _FakeApp()
    panel = PropertiesPanel(app)
    nif = NifFile()
    block = nif.add_block(
        "BSPSysSimpleColorModifier",
        {
            "Order": 2,
            "Colors": [
                {"r": 0.0, "g": 0.0, "b": 0.0, "a": 0.0},
                {"r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0},
            ],
        },
    )
    seen: dict[str, str | None] = {}

    def _capture(_block, name, _value, fdef, _schema):
        seen[name] = fdef.type if fdef is not None else None

    panel._draw_field = _capture

    panel.draw_block_fields("main", nif, block)

    assert seen["Order"] == "NiPSysModifierOrder"
    assert seen["Colors"] == "Color4"
