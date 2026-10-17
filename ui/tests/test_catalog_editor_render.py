"""Renderer tests that need no imgui frame: schema -> field mapping, defaults, summaries."""

from __future__ import annotations

import json

import pytest

from creation_lib.catalog.plugin_catalog import SCHEMAS_DIR
from ui.tools.catalog_editor import render

FEATURES = sorted(p.name.split(".")[0] for p in SCHEMAS_DIR.glob("*.schema.json") if not p.name.startswith("envelope"))
KINDS = {"string", "int", "number", "bool", "enum", "formkey", "object", "scalar_list", "formkey_list",
         "object_list", "map", "ref", "asset", "flags", "tuple", "tuple_list", "list", "variant", "tree",
         "widget:conditions", "widget:curve"}


def load(feature: str) -> dict:
    return json.loads((SCHEMAS_DIR / f"{feature}.schema.json").read_text(encoding="utf-8"))


def sections(schema: dict):
    for name, node in schema["properties"].items():
        if name != "version":
            yield name, node


def test_there_are_28_features():
    assert len(FEATURES) == 28


@pytest.mark.parametrize("feature", FEATURES)
def test_every_field_is_classified(feature):
    schema = load(feature)
    for name, node in sections(schema):
        for path, _, kind in render.walk_fields(node, schema, name):
            assert kind in KINDS, f"{feature}:{path} -> {kind}"


@pytest.mark.parametrize("feature", FEATURES)
def test_default_value_validates_for_every_section_entry(feature):
    schema = load(feature)
    checked = 0
    for name, node in sections(schema):
        n = render._simplify(schema, node)
        if render.field_kind(node, schema) == "object_list":
            entry = render._child(schema, n, n["items"])
            keys = n.get("x-key", ())
        elif render.field_kind(node, schema) == "map":
            entry = render._child(schema, n, n["additionalProperties"])
            keys = ()
        else:
            continue
        value = render.default_value(entry, schema, keys)
        assert render.validation_errors(entry, schema, value) == [], f"{feature}:{name} {value}"
        checked += 1
    assert checked or feature == "RealtimeVATS"


@pytest.mark.parametrize("feature", FEATURES)
def test_default_value_validates_for_settings_fields(feature):
    schema = load(feature)
    settings = render._simplify(schema, schema["properties"].get("settings", {}))
    for name, child in settings.get("properties", {}).items():
        node = render._child(schema, settings, child)
        value = render.default_value(node, schema)
        assert render.validation_errors(node, schema, value) == [], f"{feature}:settings.{name} {value}"


def raw_fields() -> list[str]:
    out = []
    for feature in FEATURES:
        schema = load(feature)
        for name, node in sections(schema):
            out += [f"{feature}:{p}" for p, _, k in render.walk_fields(node, schema, name) if k == "raw"]
    return out


def test_no_field_in_any_schema_is_raw_json():
    assert raw_fields() == []


SCHEMA = {
    "$id": "https://modbox21.local/catalog_schemas/Test.schema.json",
    "$defs": {"optionalForm": {"oneOf": [{"$ref": "envelope.schema.json#/$defs/formKey"}, {"type": "null"}]}},
}
FORM = {"$ref": "envelope.schema.json#/$defs/formKey"}


def kind(node: dict) -> str:
    return render.field_kind(node, SCHEMA)


def test_field_kind_scalars():
    assert kind({"type": "string"}) == "string"
    assert kind({"type": "integer"}) == "int"
    assert kind({"type": "number"}) == "number"
    assert kind({"type": "boolean"}) == "bool"
    assert kind({"enum": ["a", "b"]}) == "enum"
    assert kind({"oneOf": [{"const": "a"}, {"const": "b"}]}) == "enum"


def test_field_kind_form_keys_and_refs():
    assert kind({**FORM, "x-form-type": ["SPEL"]}) == "formkey"
    assert kind({"$ref": "#/$defs/optionalForm"}) == "formkey"
    assert kind({**FORM, "x-ref": "diseases"}) == "ref"
    assert kind({"type": "array", "items": FORM}) == "formkey_list"


def test_field_kind_containers_and_widgets():
    assert kind({"type": "object", "properties": {"a": {"type": "string"}}}) == "object"
    assert kind({"type": "object", "additionalProperties": {"type": "number"}}) == "map"
    assert kind({"type": "array", "items": {"type": "string"}}) == "scalar_list"
    assert kind({"type": "array", "items": {"type": "object", "properties": {}}}) == "object_list"
    assert kind({"type": "array", "x-widget": "curve"}) == "widget:curve"
    assert kind({"type": "object", "x-opaque": True}) == "tree"
    assert kind({"oneOf": [{"type": "string"}, {"type": "integer"}]}) == "variant"
    assert kind({}) == "tree" and kind({"type": "object"}) == "tree" and kind({"type": "array"}) == "tree"


def test_default_value_is_minimal_and_valid():
    node = {"type": "object", "required": ["form", "n"],
            "properties": {"form": FORM, "n": {"type": "integer", "minimum": 2}, "note": {"type": "string"}}}
    value = render.default_value(node, SCHEMA)
    assert value == {"form": render.PLACEHOLDER_FORM, "n": 2}
    assert render.validation_errors(node, SCHEMA, value) == []
    assert render.default_value({"$ref": "#/$defs/optionalForm"}, SCHEMA) is None


def test_default_curve_has_both_shapes():
    objects = {"type": "array", "minItems": 2, "x-widget": "curve", "items": {"type": "object", "properties": {"x": {}, "y": {}}}}
    pairs = {"type": "array", "x-widget": "curve", "items": {"type": "array", "items": {"type": "number"}}}
    assert render.default_value(objects, SCHEMA) == [{"x": 0.0, "y": 0.0}, {"x": 1.0, "y": 0.0}]
    assert render.default_value(pairs, SCHEMA) == [[0.0, 0.0], [1.0, 0.0]]


def test_summary_uses_first_scalar_fields():
    node = {"type": "object", "properties": {
        "form": FORM, "items": {"type": "array", "items": {"type": "string"}},
        "chance": {"type": "number"}, "name": {"type": "string"}, "extra": {"type": "string"}, "remove": {"type": "boolean"}}}
    entry = {"form": "A.esm:00000F", "items": ["x"], "chance": 0.5, "name": "n", "extra": "e"}
    assert render.summary(node, entry, SCHEMA, limit=3) == "form=A.esm:00000F  chance=0.5  name=n"
    assert render.summary(node, {"form": "A.esm:00000F", "remove": True}, SCHEMA) == "(removed)"
    assert render.summary(node, {"items": ["x", "y"]}, SCHEMA) == "items (2)"
    assert render.summary(node, "text", SCHEMA) == "text"


def test_curve_helpers_round_trip_both_shapes():
    from ui.tools.catalog_editor.widgets import curve

    objects = [{"x": 0, "y": 1}, {"x": 2, "y": 3}]
    pairs = [[0, 1], [2, 3]]
    assert curve.read_points(objects) == curve.read_points(pairs) == [(0.0, 1.0), (2.0, 3.0)]
    assert curve.write_points(curve.read_points(objects), True) == [{"x": 0.0, "y": 1.0}, {"x": 2.0, "y": 3.0}]
    assert curve.is_object_points({}, objects) and not curve.is_object_points({}, pairs)


def test_registry_has_the_two_widgets():
    from ui.tools.catalog_editor.widgets import REGISTRY

    assert set(REGISTRY) == {"conditions", "curve"}


# --- new generic shapes ---


def test_field_kind_new_shapes():
    assert kind({"type": "string", "x-asset": {"kind": "nif", "root": "Meshes", "strip_root": True}}) == "asset"
    assert kind({"type": "integer", "x-flags": {"1": "A", "2": "B"}}) == "flags"
    pair = {"type": "array", "items": {"type": "array", "prefixItems": [{"type": "string"}] * 2, "minItems": 2,
                                        "maxItems": 2}}
    assert kind(pair) == "tuple_list"
    assert kind({"type": "array", "prefixItems": [{"type": "string"}, {"type": "integer"}]}) == "tuple"
    assert kind({"type": "array", "items": {"type": "number"}, "minItems": 3, "maxItems": 3}) == "tuple"
    assert kind({"type": "array", "items": {"type": "array", "items": {"type": "string"}}}) == "list"
    assert kind({"type": "object", "patternProperties": {"^a": {"type": "string"}}}) == "map"
    assert kind({"type": "object", "propertyNames": FORM}) == "map"
    assert kind({"type": ["object", "null"]}) == "tree"


def test_titles_and_labels():
    assert render.humanize("max_hp") == "Max hp" and render.humanize("maxHp") == "Max Hp"
    assert render.label_of("a_b", {"title": "Nice"}) == "Nice"


def test_variant_labels_and_selection():
    radius = {"oneOf": [
        {"type": "object", "required": ["kind", "value"],
         "properties": {"kind": {"const": "literal"}, "value": {"type": "integer"}}},
        {"type": "object", "required": ["kind", "form"], "properties": {"kind": {"const": "global"}, "form": FORM}},
        {"type": "null"}, {"$ref": "envelope.schema.json#/$defs/formKey", "title": "Form"}]}
    n = render._simplify(SCHEMA, radius)
    branches = render._variants(SCHEMA, n)
    assert [render.variant_label(SCHEMA, b) for b in branches] == ["literal", "global", "None", "Form"]
    assert render.variant_index(SCHEMA, branches, {"kind": "global", "form": "A.esm:000001"}) == 1
    assert render.variant_index(SCHEMA, branches, None) == 2
    assert render.variant_index(SCHEMA, branches, "A.esm:000001") == 3
    assert render._variant_default(SCHEMA, branches[1])["kind"] == "global"
    assert render.default_value(radius, SCHEMA) == {"kind": "literal", "value": 0}


def test_flags_helpers():
    node = {"type": "integer", "x-flags": {"4": "Four", "1": "One"}}
    assert render.flag_bits(node) == [(1, "One"), (4, "Four")]
    assert render.toggle_flag(0, 4, True) == 4 and render.toggle_flag(5, 1, False) == 4


def test_rename_key_keeps_order():
    assert list(render.rename_key({"a": 1, "b": 2, "c": 3}, "b", "z")) == ["a", "z", "c"]


def test_slider_bounds_and_defaults():
    assert render._bounds({"minimum": 0, "maximum": 5}) == (0, 5)
    assert render._bounds({"exclusiveMinimum": 0}) == (1, None)
    assert render.default_value({"type": "integer", "minimum": 2, "x-flags": {"1": "a"}}, SCHEMA) == 2
    tuple_node = {"type": "array", "prefixItems": [{"type": "string"}, {"type": "integer"}]}
    assert render.default_value(tuple_node, SCHEMA) == ["", 0]


# --- headless drawing ---


@pytest.fixture
def frame_runner():
    from imgui_bundle import imgui

    context = imgui.create_context()
    io = imgui.get_io()
    io.display_size = imgui.ImVec2(1600, 1000)
    io.delta_time = 1 / 60
    io.backend_flags |= imgui.BackendFlags_.renderer_has_textures

    class Picker:
        landed: dict = {}

        def take(self, slot):
            return None

        def request(self, *args):
            pass

    def run(draw, count=2):
        for _ in range(count):
            imgui.new_frame()
            imgui.begin("test")
            result = draw()
            imgui.end()
            imgui.end_frame()
        return result

    try:
        yield run, Picker()
    finally:
        imgui.destroy_context(context)


@pytest.mark.parametrize("feature", FEATURES)
def test_every_section_and_setting_draws(feature, frame_runner):
    run, picker = frame_runner
    schema = load(feature)
    ctx = render.RenderContext(schema, None, picker, lambda section: [])
    for name, node in sections(schema):
        n = render._simplify(schema, node)
        kind = render.field_kind(node, schema)
        value = render.default_value(node, schema)
        if kind in ("object_list", "list", "tuple_list"):
            item = render._child(schema, n, n["items"])
            value = [render.default_value(item, schema, n.get("x-key", ()))]
        if kind == "map":
            value = {"Example": render.default_value(render._map_value_node(schema, n), schema)}
        changed, _ = run(lambda: render.draw_value(name, node, value, ctx, name))
        assert changed is False


def test_none_values_and_variants_draw(frame_runner):
    run, picker = frame_runner
    ctx = render.RenderContext(SCHEMA, None, picker, lambda section: [])
    nullable = {"type": ["object", "null"], "properties": {"a": {"type": "string"}}}
    assert run(lambda: render.draw_value("Maybe", nullable, None, ctx, "n")) == (False, None)
    anything = {"a": [1, "x", None, {"b": True}]}
    assert run(lambda: render.draw_value("Anything", {}, anything, ctx, "t"))[0] is False
    variant = {"oneOf": [{"type": "string", "title": "Text"}, {"type": "integer", "title": "Number"}]}
    assert run(lambda: render.draw_value("V", variant, 5, ctx, "v"))[0] is False


def test_read_only_draw_changes_nothing(frame_runner):
    run, picker = frame_runner
    ctx = render.RenderContext(SCHEMA, None, picker, lambda section: [], read_only=True)
    node = {"type": "object", "properties": {"n": {"type": "integer", "minimum": 0, "maximum": 5}}}
    assert run(lambda: render.draw_value("", node, {"n": 2}, ctx, "ro")) == (False, {"n": 2})


def test_asset_value_round_trip_through_the_landing_slot(frame_runner):
    from ui.tools.catalog_editor.assets import to_full_path, to_stored_path

    run, picker = frame_runner

    class Assets:
        def __init__(self):
            self.landed = {"mesh": "Meshes\\Weapons\\Gun.nif"}

        def take(self, slot):
            return self.landed.pop(slot, None)

        def request(self, *args):
            pass

    node = {"type": "string", "x-asset": {"kind": "nif", "root": "Meshes", "strip_root": True}}
    ctx = render.RenderContext(SCHEMA, None, picker, lambda section: [], assets=Assets())
    assert run(lambda: render.draw_value("Mesh", node, "", ctx, "mesh"), count=1) == (True, "Weapons\\Gun.nif")
    assert to_full_path("Weapons\\Gun.nif", "Meshes", True) == "Meshes\\Weapons\\Gun.nif"
    keep = {"type": "string", "x-asset": {"kind": "swf", "root": "Interface", "strip_root": False}}
    ctx.assets.landed["s"] = "interface/Menu.swf"
    assert run(lambda: render.draw_value("S", keep, "", ctx, "s"), count=1) == (True, "interface\\Menu.swf")
    assert to_stored_path("MESHES\\a\\b.nif", "Meshes", True) == "a\\b.nif"


def test_view_json_text_is_the_pretty_entry():
    from ui.tools.catalog_editor.jsonview import JsonViewer, json_text

    entry = {"form": "A.esm:000001", "n": [1, 2], "nested": {"x": None}}
    viewer = JsonViewer()
    viewer.show("title", entry)
    assert viewer._text == json_text(entry) == json.dumps(entry, indent=2)
