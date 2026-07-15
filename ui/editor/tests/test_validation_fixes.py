"""Tests for validation panel automatic fixes."""

from types import SimpleNamespace
from unittest.mock import MagicMock

from ui.editor.panels.validation import ValidationPanel


class _FakeBlock:
    def __init__(self, type_name: str, block_id: int, fields: dict):
        self.type_name = type_name
        self.block_id = block_id
        self._field_values = dict(fields)
        self.fields = list(fields.items())

    def get_field(self, name: str):
        return self._field_values.get(name)

    def set_field(self, name: str, value):
        self._field_values[name] = value
        for i, (field_name, _) in enumerate(self.fields):
            if field_name == name:
                self.fields[i] = (name, value)
                break
        else:
            self.fields.append((name, value))

    def get_refs(self, schema):
        return []


class _FakeSchema:
    def __init__(self, ref_fields=None):
        self.ref_fields = ref_fields or {}

    def get_all_fields(self, type_name: str):
        return self.ref_fields.get(type_name, [])

    def is_subtype_of(self, type_name: str, base: str):
        return type_name == base


class _FakeNif:
    def __init__(self, blocks, schema):
        self.blocks = blocks
        self.schema = schema
        self.header = SimpleNamespace(bs_version=130)

    def get_block(self, block_id: int):
        return self.blocks[block_id]


class _FakeRegistry:
    active_id = "main"


class _FakeApp:
    def __init__(self, nif):
        self.nif_file = nif
        self.registry = _FakeRegistry()
        self.undo_manager = MagicMock()
        self.rebuild_scene_from_nif = MagicMock()
        self._nif_dirty = False
        self.status_text = ""


def test_fix_issues_sanitizes_broken_refs_and_records_undo():
    ref_field = SimpleNamespace(name="Target", suffix=None, type="Ref")
    block = _FakeBlock("NiNode", 0, {"Target": 99})
    nif = _FakeNif([block], _FakeSchema({"NiNode": [ref_field]}))
    app = _FakeApp(nif)
    panel = ValidationPanel(app)

    panel.validate()
    assert panel._issues

    panel.fix_issues()

    assert block.get_field("Target") == -1
    assert app.undo_manager.push.call_count == 1
    assert app._nif_dirty is True
    app.rebuild_scene_from_nif.assert_called_once()
    assert app.status_text == "Fixed 1 validation issue(s)"


def test_fix_validation_issues_normalizes_prunes_and_uniquifies_names():
    shape_a = _FakeBlock(
        "BSTriShape",
        0,
        {
            "Name": "SharedName",
            "Vertex Data": [{"Normal": {"x": 2.0, "y": 0.0, "z": 0.0}}],
            "Triangles": [{"v1": 0, "v2": 0, "v3": 1}, {"v1": 0, "v2": 1, "v3": 2}],
            "Num Triangles": 2,
        },
    )
    shape_b = _FakeBlock("BSTriShape", 1, {"Name": "SharedName"})
    nif = _FakeNif([shape_a, shape_b], _FakeSchema())
    panel = ValidationPanel(MagicMock())

    fixed = panel._fix_validation_issues(nif)

    normal = shape_a.get_field("Vertex Data")[0]["Normal"]
    assert normal == {"x": 1.0, "y": 0.0, "z": 0.0}
    assert shape_a.get_field("Triangles") == [{"v1": 0, "v2": 1, "v3": 2}]
    assert shape_a.get_field("Num Triangles") == 1
    assert shape_b.get_field("Name") == "SharedName_2"
    assert fixed == 3
