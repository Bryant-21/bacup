"""Disposable SWF/AS3 fixtures: persistence, diagnostics and interrupted work."""
from __future__ import annotations

import copy
import json
import struct
from threading import Event

import pytest
from click.testing import CliRunner

from cli.main import cli
from creation_lib.swf import native_runtime
from creation_lib.swf.parser import parse_swf
from creation_lib.swf.project import build_document, load_project_file
from creation_lib.swf.svg_io import svg_to_shapes
from creation_lib.swf.writer import write_swf
from ui.swf_editor.document import RevisionJob, SwfEditingDocument, atomic_write
from ui.swf_editor.movie import Movie, MovieTag, encode_tag
from ui.swf_editor.swf_editor_app import SwfEditorApp
from ui.swf_editor.swf_scene import AffineTransform, EditorDisplayEntry


SOURCE = '''package { import flash.display.MovieClip;
public class B21_Fixture extends MovieClip {
public function B21_Fixture() {} public function value():int { return 7; }
} }'''
REPLACEMENT = SOURCE.replace("return 7", "return 42")
SVG = '<svg xmlns="http://www.w3.org/2000/svg"><rect width="40" height="30" fill="#ffffff"/></svg>'


def artwork():
    doc = SwfEditingDocument()
    shape = svg_to_shapes(SVG)[0]
    doc.scene.library_symbols[shape.shape_id] = shape
    doc.scene.layers[0].keyframes[0].display_list.append(
        EditorDisplayEntry(shape.shape_id, AffineTransform(x=75, y=55, scale_x=2, rotation=30)))
    return doc


def compiled():
    data = write_swf(build_document({"canvas": [128, 64], "scripts": ["Fixture.as"],
                                    "script_sources": {"Fixture.as": SOURCE},
                                    "exports": [{"character": 0, "class": "B21_Fixture"}]}, "."))
    movie = Movie(data)
    opaque = MovieTag(222, b"keep opaque\x00\xff", encode_tag(222, b"keep opaque\x00\xff"))
    return movie.assemble(movie.tags[:-1] + [opaque, movie.tags[-1]])


def test_visual_project_is_roundtrip_and_cli_packable(tmp_path):
    doc = artwork()
    doc.scene.layers[0].keyframes[0].duration = 4
    doc.scene.insert_keyframe(2)
    doc.scene.layers[0].keyframes[1].display_list[0].transform.x = 150
    doc.scene.layers[0].keyframes[1].label = "end"
    doc.scene.background.r = 12
    path = tmp_path / "art.swfproj"
    doc.save(path)
    reopened = SwfEditingDocument.open(path)
    assert reopened.mode == "visual"
    assert reopened.scene.to_project() == doc.scene.to_project()
    assert not reopened.dirty
    packed = write_swf(load_project_file(path))
    assert packed == reopened.build()
    result = CliRunner().invoke(cli, ["swf", "pack", str(path), "-o", str(tmp_path / "packed.swf")])
    assert result.exit_code == 0, result.output
    assert (tmp_path / "packed.swf").read_bytes() == packed
    parsed = parse_swf(packed)
    assert parsed.header.frame_count == 4
    assert parsed.main_timeline.display_list_at(0)[1].matrix.translate_x == 1500
    assert parsed.main_timeline.display_list_at(2)[1].matrix.translate_x == 3000
    assert parsed.main_timeline.frames[2].label == "end"
    assert parsed.background_color.r == 12


def test_transform_retains_rotation_scale_shear_reflection():
    transform = AffineTransform(x=7, y=-3, scale_x=2, scale_y=-4, rotation=61, skew_x=0.25)
    before = transform.to_matrix()
    after = AffineTransform.from_matrix(before).to_matrix()
    for field in ("scale_x", "scale_y", "rotate_skew_0", "rotate_skew_1"):
        assert getattr(after, field) == pytest.approx(getattr(before, field))
    assert transform.inverse(*transform.apply(11, 12)) == pytest.approx((11, 12))


def test_export_writes_and_reopen_then_save_does_not_revert_art(tmp_path):
    original = tmp_path / "original.swf"
    original.write_bytes(artwork().build())
    doc = SwfEditingDocument.open(original)
    assert doc.mode == "visual"
    assert doc.build() == original.read_bytes()
    doc.scene.layers[0].keyframes[0].display_list[0].transform.x = 333
    doc.save(tmp_path / "edited.swfproj")
    out = tmp_path / "edited.swf"
    doc.export(out)
    assert parse_swf(out.read_bytes()).main_timeline.display_list_at(0)[1].matrix.translate_x == 6660
    assert original.read_bytes() != out.read_bytes()
    with pytest.raises(ValueError, match="preserve"):
        doc.export(original)


def test_as3_buffers_save_compile_and_diagnostics_preserve_external_source(tmp_path):
    (tmp_path / "Fixture.as").write_text(SOURCE)
    path = tmp_path / "source.swfproj"
    path.write_text(json.dumps({"scripts": ["Fixture.as"], "exports": [{"character": 0, "class": "B21_Fixture"}]}))
    doc = SwfEditingDocument.open(path)
    doc.sources["Fixture.as"] = REPLACEMENT
    assert doc.dirty
    doc.save(tmp_path / "saved.swfproj")
    reopened = SwfEditingDocument.open(tmp_path / "saved.swfproj")
    report = native_runtime.abc_disassemble(reopened.build(), "B21_Fixture", "value")
    assert "42" in json.dumps(report)
    output = tmp_path / "packed-source.swf"
    result = CliRunner().invoke(cli, ["swf", "pack", str(reopened.path), "-o", str(output)])
    assert result.exit_code == 0, result.output
    assert "42" in json.dumps(native_runtime.abc_disassemble(output.read_bytes(), "B21_Fixture", "value"))
    assert (tmp_path / "Fixture.as").read_text() == SOURCE
    reopened.sources["Fixture.as"] = "package { public class }"
    with pytest.raises(Exception, match="identifier"):
        reopened.build()
    reopened.save(tmp_path / "invalid-source.swfproj")
    assert "public class }" in SwfEditingDocument.open(tmp_path / "invalid-source.swfproj").sources["Fixture.as"]


def test_invalid_manifest_can_be_saved_reopened_and_repaired(tmp_path):
    doc = SwfEditingDocument.menu()
    doc.manifest_buffer = '{"scripts":'
    doc.save(tmp_path / "broken.swfproj")
    reopened = SwfEditingDocument.open(tmp_path / "broken.swfproj")
    assert reopened.manifest_buffer == '{"scripts":'
    with pytest.raises(json.JSONDecodeError):
        reopened.build()
    reopened.manifest_buffer = json.dumps(reopened.manifest)
    assert "DocumentMenu" in native_runtime.abc_class_names(reopened.build())


def test_binary_noop_and_class_edit_preserve_opaque_tags_and_original(tmp_path):
    path = tmp_path / "menu.swf"
    original = compiled()
    path.write_bytes(original)
    doc = SwfEditingDocument.open(path)
    assert doc.mode == "binary"
    assert doc.build() == original
    before = [tag.encoded for tag in Movie(original).tags if tag.code not in {72, 82}]
    doc.apply_sources({"B21_Fixture": REPLACEMENT})
    after = [tag.encoded for tag in Movie(doc.movie).tags if tag.code not in {72, 82}]
    assert before == after
    assert "42" in json.dumps(doc.inspect_class("B21_Fixture", "disassembly", "value"))
    doc.save(tmp_path / "session.swfproj")
    reopened = SwfEditingDocument.open(tmp_path / "session.swfproj")
    assert reopened.build() == doc.movie
    assert (reopened.scene.canvas_width, reopened.scene.canvas_height) == (128, 64)
    assert reopened.scene.fps == doc.scene.fps
    assert reopened.scene.background == doc.scene.background
    with pytest.raises(ValueError, match="preserve"):
        SwfEditingDocument.open(tmp_path / "session.swfproj").export(path)
    with pytest.raises(ValueError, match="Binary SWF edit"):
        load_project_file(tmp_path / "session.swfproj")
    assert path.read_bytes() == original
    previous = doc.movie
    with pytest.raises(Exception):
        doc.apply_sources({"B21_Fixture": "broken source"})
    assert doc.movie == previous


def test_raw_tag_replacement_retains_other_tags_and_rejects_truncation():
    original = Movie(compiled())
    index = next(i for i, tag in enumerate(original.tags) if tag.code == 222)
    result = Movie(original.replace_tag(index, b"replaced"))
    assert result.tags[index].payload == b"replaced"
    assert all(a.encoded == b.encoded for i, (a, b) in enumerate(zip(original.tags, result.tags)) if i != index)
    with pytest.raises(ValueError, match="End"):
        original.replace_tag(len(original.tags) - 1, b"bad")
    with pytest.raises(ValueError):
        Movie(original.original[:-4])


def test_async_stale_cancelled_and_repeated_builds_cannot_commit():
    doc = SwfEditingDocument.menu()
    jobs = RevisionJob()
    release = Event()
    try:
        jobs.start(doc, lambda _: (release.wait(2), b"old")[1])
        with pytest.raises(RuntimeError, match="already"):
            jobs.start(doc, lambda _: b"duplicate")
        doc.sources["DocumentMenu.as"] += "\n// changed"
        release.set()
        jobs.future.result(timeout=5)
        assert jobs.poll(doc)[0] == "stale"
        jobs.start(doc, lambda _: b"cancelled")
        jobs.cancel()
        assert jobs.poll(doc) is None
        jobs.start(doc, lambda _: b"current")
        jobs.future.result(timeout=5)
        assert jobs.poll(doc) == ("build", b"current")
    finally:
        release.set()
        jobs.close()


def test_undo_redo_geometry_and_unsaved_open_cancellation(tmp_path):
    app = SwfEditorApp()
    try:
        initial = app.scene.to_project()
        app.push_undo("Draw")
        app.document.scene = artwork().scene
        assert app.document.dirty
        drawn = app.scene.to_project()
        app.undo()
        assert app.scene.to_project() == initial
        assert not app.document.dirty
        app.redo()
        assert app.scene.to_project() == drawn
        path = tmp_path / "replacement.swf"
        path.write_bytes(compiled())
        app.open_document(path)
        assert app.document.mode == "visual"
        first_action = app._pending_action
        app.new_document(menu=True)
        assert app._pending_action is first_action
        app.resolve_pending("cancel")
        assert app.scene.to_project() == drawn
        app.open_document(path)
        app.resolve_pending("discard")
        assert app.document.mode == "binary"
        app.open_document(tmp_path / "missing.swf")
        assert app.document.mode == "binary"
        assert app.status_text.startswith("Error")
    finally:
        app.cleanup()


def test_failed_save_is_atomic_and_does_not_clear_dirty(tmp_path, monkeypatch):
    doc = artwork()
    destination = tmp_path / "existing.swfproj"
    destination.write_bytes(b"original")
    import ui.swf_editor.document as module
    def fail(*_):
        raise OSError("fixture replacement denied")
    monkeypatch.setattr(module.os, "replace", fail)
    with pytest.raises(OSError):
        doc.save(destination)
    assert destination.read_bytes() == b"original"
    assert doc.dirty
    assert list(tmp_path.iterdir()) == [destination]


def test_shape_replacement_preserves_unrelated_menu_data():
    doc = artwork()
    data = Movie(doc.build())
    tags = data.tags[:-1] + [MovieTag(222, b"opaque", encode_tag(222, b"opaque")), data.tags[-1]]
    data = Movie(data.assemble(tags))
    replacement = svg_to_shapes(SVG.replace('width="40"', 'width="80"'))[0]
    before = [tag.encoded for tag in data.tags if tag.code not in {2, 22, 32, 83}]
    replaced = Movie(data.replace_shape(1, replacement))
    assert before == [tag.encoded for tag in replaced.tags if tag.code not in {2, 22, 32, 83}]
    assert parse_swf(replaced.original).shapes[1].bounds[2] == 1600


def test_stage_remove_hold_alpha_and_missing_geometry():
    doc = artwork()
    doc.scene.layers[0].opacity = 0.5
    doc.scene.layers[0].keyframes[0].duration = 3
    doc.scene.insert_keyframe(2, blank=True)
    parsed = parse_swf(doc.build())
    assert parsed.main_timeline.display_list_at(0)[1].color_transform is not None
    assert parsed.main_timeline.display_list_at(2) == {}
    broken = copy.deepcopy(doc)
    broken.scene.library_symbols.clear()
    with pytest.raises(ValueError, match="Missing vector"):
        broken.build()


def test_declared_export_must_have_a_compiled_class():
    doc = SwfEditingDocument.menu()
    project = json.loads(doc.manifest_buffer)
    project["exports"] = [{"character": 0, "class": "Missing"}]
    doc.manifest_buffer = json.dumps(project)
    with pytest.raises(ValueError, match="Missing"):
        doc.build()


def test_container_rejects_invalid_length_and_unsupported_signature():
    data = bytearray(compiled())
    struct.pack_into("<I", data, 4, 9)
    with pytest.raises(ValueError, match="length"):
        Movie(data)
    with pytest.raises(ValueError, match="ZWS"):
        Movie(b"ZWS" + bytes(30))


def test_atomic_write_requires_existing_destination_directory(tmp_path):
    with pytest.raises(OSError):
        atomic_write(tmp_path / "missing" / "out.swf", b"bytes")
    assert not (tmp_path / "missing").exists()


def test_save_choice_failure_cancel_and_successful_transition(tmp_path, monkeypatch):
    from importlib import import_module
    pick_folder = import_module("creation_lib.ui.widgets.pick_folder")
    app = SwfEditorApp()
    try:
        app.document.scene = artwork().scene
        app.new_document(menu=True)
        monkeypatch.setattr(pick_folder, "pick_save_file", lambda *a, **kw: None)
        assert not app.resolve_pending("save")
        assert app._pending_action is not None
        monkeypatch.setattr(pick_folder, "pick_save_file", lambda *a, **kw: str(tmp_path / "saved.swfproj"))
        assert app.resolve_pending("save")
        assert app.document.mode == "source"
        assert (tmp_path / "saved.swfproj").is_file()
    finally:
        app.cleanup()


def test_blank_tail_and_frame_labels_survive_edit_export(tmp_path):
    data = write_swf(build_document({"stage": [{"label": "start"}, {}, {"label": "end"}]}, "."))
    path = tmp_path / "blank.swf"
    path.write_bytes(data)
    doc = SwfEditingDocument.open(path)
    doc.scene.background.r = 1
    parsed = parse_swf(doc.build())
    assert parsed.header.frame_count == 3
    assert parsed.main_timeline.frames[0].label == "start"
    assert parsed.main_timeline.frames[2].label == "end"


def test_attached_as3_to_unchanged_artwork_is_actually_compiled(tmp_path):
    path = tmp_path / "art.swf"
    path.write_bytes(artwork().build())
    doc = SwfEditingDocument.open(path)
    doc.sources["Fixture.as"] = SOURCE
    doc.manifest["exports"] = [{"character": 0, "class": "B21_Fixture"}]
    assert "B21_Fixture" in native_runtime.abc_class_names(doc.build())


def test_opacity_animation_can_reopen_as_editable_artwork(tmp_path):
    doc = artwork()
    first = doc.scene.layers[0].keyframes[0]
    first.duration = 3
    doc.scene.insert_keyframe(1)
    doc.scene.layers[0].keyframes[1].display_list[0].color_alpha = 128
    path = tmp_path / "alpha.swf"
    path.write_bytes(doc.build())
    reopened = SwfEditingDocument.open(path)
    assert reopened.mode == "visual"
    assert len(reopened.scene.layers[0].keyframes) == 2
    original_alpha = parse_swf(path.read_bytes()).main_timeline.display_list_at(1)[1].color_transform
    reopened.scene.layers[0].keyframes[0].display_list[0].transform.x += 1
    rebuilt_alpha = parse_swf(reopened.build()).main_timeline.display_list_at(1)[1].color_transform
    assert rebuilt_alpha == original_alpha


def test_fractional_rate_and_shape4_flags_remain_opaque(tmp_path):
    movie = Movie(artwork().build())
    header = bytearray(movie.header)
    struct.pack_into("<H", header, len(header) - 4, 30 * 256 + 128)
    body = bytes(header) + b"".join(tag.encoded for tag in movie.tags)
    data = b"FWS" + bytes([movie.version]) + struct.pack("<I", len(body) + 8) + body
    path = tmp_path / "fractional.swf"
    path.write_bytes(data)
    doc = SwfEditingDocument.open(path)
    assert doc.mode == "binary" and doc.scene.fps == 30.5
    assert doc.build() == data
    from creation_lib.swf.tags import DefineShapeTag
    shape = artwork().scene.library_symbols[1]
    shape.shape_version = 4
    shape.edge_bounds = shape.bounds
    payload = bytearray(DefineShapeTag(shape, 83).to_bytes())
    offset = 2
    for _ in range(2):
        offset += (5 + 4 * (payload[offset] >> 3) + 7) // 8
    payload[offset] = 1
    tags = [MovieTag(83, bytes(payload), encode_tag(83, bytes(payload))) if tag.code in {2, 22, 32, 83}
            else tag for tag in movie.tags]
    path = tmp_path / "shape4.swf"
    path.write_bytes(movie.assemble(tags))
    doc = SwfEditingDocument.open(path)
    assert doc.mode == "binary" and doc.build() == path.read_bytes()


def test_workspace_drop_opens_swf_and_project_with_unsaved_guard(tmp_path):
    from ui.toolkit.workspaces.swf_editor_workspace import SwfEditorWorkspace
    app = SwfEditorApp()
    workspace = SwfEditorWorkspace()
    workspace._app = app
    swf = tmp_path / "drop.swf"
    swf.write_bytes(compiled())
    project = tmp_path / "drop.swfproj"
    SwfEditingDocument.menu().save(project)
    try:
        assert workspace.handle_file_drop([str(swf)])
        assert app.document.mode == "binary"
        app.document.sources["Fixture.as"] = SOURCE
        assert workspace.handle_file_drop([str(project)])
        assert app.document.mode == "binary" and app._pending_action is not None
        app.resolve_pending("cancel")
        assert app.document.sources["Fixture.as"] == SOURCE
        assert workspace.handle_file_drop([str(project)])
        app.resolve_pending("discard")
        assert app.document.mode == "source" and app.document.path == project
        assert not workspace.handle_file_drop([str(tmp_path / "unrelated.nif")])
    finally:
        app.cleanup()
