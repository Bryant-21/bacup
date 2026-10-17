"""Actual tool gestures, geometry and OpenGL output on disposable artwork."""
from __future__ import annotations

import json

from imgui_bundle import imgui
import moderngl
import numpy as np
import pytest
from pyglm import glm

from creation_lib.preprocessor.swf import serialize_shape_records
from creation_lib.swf import native_runtime
from creation_lib.swf.project import build_document
from creation_lib.swf.svg_io import svg_to_shapes
from creation_lib.swf.writer import write_swf
from ui.swf_editor.swf_editor_app import SwfEditorApp
from ui.swf_editor.swf_renderer import SwfRenderer, _tessellate_fills
from ui.swf_editor.swf_scene import AffineTransform, EditorDisplayEntry
from ui.tests.test_swf_editor_workflow import SVG, artwork, compiled


@pytest.fixture
def app():
    context = imgui.create_context()
    editor = SwfEditorApp()
    editor._init_tools()
    editor.camera.viewport_width, editor.camera.viewport_height = 550, 400
    editor.camera.zoom = 1
    try:
        yield editor
    finally:
        editor.cleanup()
        imgui.destroy_context(context)


def test_draw_primitives_export_and_undo(app):
    for name in ("rect", "ellipse", "line"):
        tool = app._tools[name]
        tool.on_mouse_down(50, 60, 0)
        tool.on_mouse_move(150, 130)
        tool.on_mouse_up(150, 130, 0)
    assert len(app.scene.library_symbols) == 3
    assert all(_tessellate_fills(s) for s in app.scene.library_symbols.values())
    before = app.document.build()
    app.undo()
    assert len(app.scene.library_symbols) == 2
    app.redo()
    assert app.document.build() == before


def test_pen_path_and_cancel_without_committing(app):
    pen = app._tools["pen"]
    for x, y in ((50, 50), (100, 50), (80, 100), (50, 50)):
        pen.on_mouse_down(x, y, 0)
        pen.on_mouse_up(x, y, 0)
    assert len(app.scene.library_symbols) == 1
    pen.on_mouse_down(200, 200, 0)
    pen.on_key(imgui.Key.escape, True)
    assert len(app.scene.library_symbols) == 1
    assert app.document.build()


def test_select_transformed_shape_drag_and_cancel(app):
    app.document.scene = artwork().scene
    tool = app._tools["select"]
    entry = app.scene.layers[0].keyframes[0].display_list[0]
    x, y = entry.transform.apply(20, 15)
    assert tool.on_mouse_down(x, y, 0)
    assert app.scene.selection == [(0, 0)]
    tool.on_mouse_move(x + 25, y + 10)
    tool.on_mouse_up(x + 25, y + 10, 0)
    assert entry.transform.x == 100
    app.undo()
    assert app.scene.layers[0].keyframes[0].display_list[0].transform.x == 75


def test_fill_eyedropper_library_placement_and_history(app):
    shape = svg_to_shapes(SVG)[0]
    blob = serialize_shape_records(shape)
    app.place_shape({"shape_data": blob, "bounds": "[0,0,40,30]", "id": 1})
    assert len(app.scene.library_symbols) == 1
    fill = app._tools["fill"]
    from creation_lib.swf.types import RGBA
    fill.active_color = RGBA(255, 0, 0, 255)
    fill.on_mouse_down(10, 10, 0)
    assert app.scene.library_symbols[1].fill_styles[0].color.g == 0
    app._tools["eyedropper"].on_mouse_down(10, 10, 0)
    assert fill.active_color.g == 0
    app.undo()
    assert app.scene.library_symbols[1].fill_styles[0].color.g == 255
    app.undo()
    assert not app.scene.library_symbols
    app.redo()
    assert len(app.scene.library_symbols) == 1


@pytest.mark.parametrize("operation", ["union", "subtract", "intersect", "exclude"])
def test_boolean_operations_respect_placements_and_can_undo(app, operation):
    app.document.scene = artwork().scene
    entry = app.scene.layers[0].keyframes[0].display_list[0]
    entry.transform = AffineTransform(x=10, y=10)
    app.scene.layers[0].keyframes[0].display_list.append(EditorDisplayEntry(1, AffineTransform(x=30, y=10)))
    app.scene.selection = [(0, 0), (0, 1)]
    before = app.scene.to_project()
    app.boolean_op(operation)
    assert app.scene.to_project() != before
    assert app.document.build()
    app.undo()
    assert app.scene.to_project() == before


def test_svg_and_raster_import_are_undoable(app, tmp_path):
    from PIL import Image
    svg = tmp_path / "fixture.svg"
    svg.write_text(SVG)
    app.import_art(svg)
    app.undo()
    assert not app.scene.library_symbols
    image = np.zeros((80, 80, 4), dtype=np.uint8)
    image[20:60, 20:60] = [255, 255, 255, 255]
    raster = tmp_path / "fixture.png"
    Image.fromarray(image).save(raster)
    app.import_art(raster)
    assert app.scene.library_symbols
    assert app.document.build()


def test_gpu_output_applies_placement_alpha_and_background():
    ctx = moderngl.create_standalone_context(require=330)
    renderer = SwfRenderer(ctx)
    try:
        renderer.ensure_fbo(550, 400)
        shape = svg_to_shapes(SVG)[0]
        matrix = SwfEditorApp._gl_transform(AffineTransform(x=75, y=55, scale_x=2))
        projection = glm.ortho(0, 550, 400, 0, -1, 1)
        renderer.render(projection, [(shape, matrix, 0.5)], show_grid=False,
                        canvas_width=550, canvas_height=400, canvas_color=(0, 0, 0))
        pixels = np.frombuffer(renderer.fbo.read(components=4), np.uint8).reshape(400, 550, 4)[::-1]
        assert pixels[60, 90, 0] in range(125, 131)
        assert pixels[10, 10, 0] == 0
        assert pixels[60, 200, 0] == 0
        renderer.render(projection, [], show_grid=False, canvas_width=550, canvas_height=400,
                        canvas_color=(1, 0, 0))
        pixels = np.frombuffer(renderer.fbo.read(components=4), np.uint8).reshape(400, 550, 4)
        assert tuple(pixels[20, 20, :3]) == (255, 0, 0)
    finally:
        renderer.cleanup()
        ctx.release()


def test_compiled_class_member_merge_and_symbol_injection():
    original = compiled()
    merge = '''package { import flash.display.MovieClip;
    public class B21_Fixture extends MovieClip { public function B21_Fixture() {}
    public function extra():int { return 99; } } }'''
    result = native_runtime.augment_as3_classes(original, {"B21_Fixture": merge})
    report = json.dumps(native_runtime.abc_disassemble(result, "B21_Fixture"))
    assert "extra" in report and "value" in report
    donor = artwork().scene.to_authoring_project()
    donor["exports"] = [{"character": 1, "class": "B21_Art"}]
    donor_bytes = write_swf(build_document(donor, "."))
    injected = native_runtime.inject_symbols(donor_bytes, original, ["B21_Art"])
    assert any(name == "B21_Art" for _, name in native_runtime.list_symbols(injected))
    png = native_runtime.render_symbol_png(injected, "B21_Art")
    assert png[:8] == b"\x89PNG\r\n\x1a\n"


def test_advanced_abc_patch_and_namespace_rename_preserve_opaque_data():
    source = '''package { import flash.display.MovieClip;
    public class B21_Fixture extends MovieClip { public function B21_Fixture() {}
    public function value():int { return before(); }
    public function before():int { return 7; } public function after():int { return 42; }
    } }'''
    original = native_runtime.replace_as3_classes(compiled(), {"B21_Fixture": source})
    pattern = [["callproperty", "before", 0]]
    replacement = [["callproperty", "after", 0]]
    patched = native_runtime.patch_as3_method(original, "B21_Fixture", "value", pattern, replacement,
                                             expected_matches=1)
    report = json.dumps(native_runtime.abc_disassemble(patched, "B21_Fixture", "value"))
    assert "after" in report and "before" not in report
    with pytest.raises(Exception, match="found 0"):
        native_runtime.patch_as3_method(patched, "B21_Fixture", "value", pattern, replacement)
    renamed = native_runtime.rename_as3_classes(patched, "B21_Isolated")
    assert native_runtime.abc_class_names(renamed) == ["B21_Isolated_B21_Fixture"]
    assert native_runtime.list_symbols(renamed) == [(0, "B21_Isolated_B21_Fixture")]
    from ui.swf_editor.movie import Movie
    assert [t.encoded for t in Movie(original).tags if t.code not in {76, 82}] == [
        t.encoded for t in Movie(renamed).tags if t.code not in {76, 82}]


def test_create_remove_and_undo_as3_class_source(app):
    from ui.swf_editor.document import SwfEditingDocument
    from ui.swf_editor.panels.document_panel import DocumentPanel
    app._replace_document(SwfEditingDocument.menu())
    panel = DocumentPanel(app)
    panel.new_source()
    assert "B21_NewClass" in native_runtime.abc_class_names(app.document.build())
    with pytest.raises(ValueError, match="already attached"):
        panel.new_source()
    panel.remove_source()
    assert "B21_NewClass.as" not in app.document.sources
    app.undo()
    assert "B21_NewClass.as" in app.document.sources
    app.undo()
    assert "B21_NewClass.as" not in app.document.sources


def test_cancelled_gesture_restores_shape_and_undo(app):
    app.document.scene = artwork().scene
    before = app.document.state_key()
    app._gesture_snapshot = app._take_snapshot()
    app._gesture_undo_count = len(app.undo_stack._undo)
    app._captured_tool = app._tools["select"]
    app.push_undo("Move")
    app.scene.layers[0].keyframes[0].display_list[0].transform.x += 100
    app.cancel_gesture()
    assert app.document.state_key() == before
    assert not app.undo_stack.can_undo


def test_outline_stroke_builds_filled_geometry_and_undo(app):
    from creation_lib.swf.types import LineStyle, RGBA
    from creation_lib.swf.shapes import StyleChange
    app.document.scene = artwork().scene
    shape = app.scene.library_symbols[1]
    shape.line_styles = [LineStyle(width=60, color=RGBA(0, 0, 0, 255))]
    next(r for r in shape.records if isinstance(r, StyleChange)).line = 1
    app.scene.selection = [(0, 0)]
    before = app.scene.to_project()
    app.outline_stroke()
    outlined_id = app.scene.layers[0].keyframes[0].display_list[0].shape_id
    assert not app.scene.library_symbols[outlined_id].line_styles
    assert len(app.scene.library_symbols[outlined_id].fill_styles) == 2
    assert app.document.build()
    app.undo()
    assert app.scene.to_project() == before


def test_cubic_conversion_subdivides_and_preserves_exact_endpoint():
    from ui.swf_editor.geometry import cubic_to_quadratic_records
    edges = cubic_to_quadratic_records((0, 0), (0, 100), (100, -100), (100, 0))
    assert len(edges) > 1
    assert sum(e.cx + e.ax for e in edges) == 2000
    assert sum(e.cy + e.ay for e in edges) == 0
