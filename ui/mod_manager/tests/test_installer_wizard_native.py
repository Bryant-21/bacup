"""Actual wizard repair/retry/layout controls on generated payloads."""
from pathlib import Path

from imgui_bundle import imgui
from PIL import Image
import pytest

from . import test_widget_ids as widgets
from .test_installer_repairs import repair_package

native, workspace = widgets.native, widgets.workspace


@pytest.mark.parametrize("size,scale", [((1280, 800), 1.0), ((1600, 900), 1.25), ((1280, 800), 1.5)])
def test_same_package_source_repairs_review_and_install_via_input(native, workspace, tmp_path, size, scale):
    native.io.display_size = size
    imgui.get_style().font_size_base = 16
    imgui.get_style().font_scale_main = scale
    root = repair_package(tmp_path / "repair package")
    draw = widgets.panels(workspace)
    workspace._prepare(str(root))
    widgets.finish_job(workspace, native, draw)
    m = workspace.manager
    for original, replacement in (("missing.txt", "replacement.txt"), ("missing-folder", "alternative")):
        for _ in range(12):
            if any(item.label == "Bind missing source" for item in native.items):
                break
            native.scroll(native.find("Missing source repair"), draw, -2)
        field = native.find("##missing_declared_source")
        native.text(field, draw, original)
        native.text(native.find("##existing_package_payload"), draw, replacement)
        native.click(native.find("Bind missing source"), draw)
        widgets.finish_job(workspace, native, draw)
    assert m.install_repairs == {"missing.txt": "replacement.txt", "missing-folder": "alternative"}
    native.click(native.find("Review destination files"), draw)
    widgets.finish_job(workspace, native, draw)
    button = native.find("Install into library")
    assert button.rect[3] <= size[1]
    native.click(button, draw)
    widgets.finish_job(workspace, native, draw)
    installed = Path(m.rows_by_id[m.selected_id].source)
    assert (installed / "textures/missing.txt").read_bytes() == b"generated replacement"
    assert (installed / "meshes/file.txt").read_bytes() == b"generated folder replacement"
    assert (root / "replacement.txt").read_bytes() == b"generated replacement"


@pytest.mark.parametrize("size,scale", [((1280, 800), 1.5), ((1600, 900), 1.0)])
def test_required_conflict_retry_manual_review_and_cancel_keep_sources(native, workspace, tmp_path, size, scale):
    native.io.display_size = size
    imgui.get_style().font_scale_main = scale
    root = repair_package(tmp_path / "conflict", conflict=True)
    original = (root / "fomod/ModuleConfig.xml").read_bytes()
    draw = widgets.panels(workspace)
    workspace._prepare(str(root))
    widgets.finish_job(workspace, native, draw)
    native.click(native.find("Next"), draw)
    assert workspace._step == 0
    native.click(native.find("Reset current step choices"), draw)
    assert workspace._evaluation.errors and workspace._step == 0
    native.click(native.find("Reopen package and dependencies"), draw)
    widgets.finish_job(workspace, native, draw)
    assert workspace._evaluation.errors
    native.click(native.find("Reopen in manual layout"), draw)
    widgets.finish_job(workspace, native, draw)
    native.click(native.find("Review destination files"), draw)
    widgets.finish_job(workspace, native, draw)
    assert workspace.manager.install_plan and workspace.manager.prepared.manual
    native.click(native.find("Close installer"), draw)
    assert workspace.manager.prepared is None
    assert (root / "fomod/ModuleConfig.xml").read_bytes() == original


@pytest.mark.parametrize("size,scale", [((1280, 800), 1.5), ((1600, 900), 1.0)])
def test_decorated_header_is_bounded_and_footer_visible(native, workspace, tmp_path, monkeypatch, size, scale):
    # This input harness has no OpenGL renderer. Reserve the real image's bounds;
    # backend screenshots separately exercise its texture upload and rendering.
    from imgui_bundle import immvision
    displayed = []
    def reserve_image(label, pixels, image_size, **kwargs):
        displayed.append((pixels.shape, image_size))
        imgui.dummy(image_size)
    monkeypatch.setattr(immvision, "image_display", reserve_image)
    native.io.display_size = size
    imgui.get_style().font_scale_main = scale
    root = repair_package(tmp_path / "decorated")
    Image.new("RGBA", (320, 180), (180, 80, 20, 255)).save(root / "fomod/header.png")
    xml = (root / "fomod/ModuleConfig.xml").read_text().replace('<moduleName>', '<moduleName position="RightOfImage" colour="F4B860">')
    xml = xml.replace('</moduleName>', '</moduleName><moduleImage path="fomod/header.png" showFade="true" height="10000"/>')
    (root / "fomod/ModuleConfig.xml").write_text(xml)
    draw = widgets.panels(workspace)
    workspace._prepare(str(root))
    widgets.finish_job(workspace, native, draw)
    assert workspace.manager.prepared.header_image.shape[:2] == (180, 320)
    assert displayed and displayed[-1][1][1] <= 240 * scale
    assert native.find("Review destination files").rect[3] <= size[1]
    assert native.find("Close installer").rect[3] <= size[1]
    native.click(native.find("Close installer"), draw)
    assert workspace.manager.prepared is None


@pytest.mark.parametrize("size,scale", [((1280, 800), 1.5), ((1600, 900), 1.0)])
def test_referenced_additive_update_destination_review_and_install_via_input(native, workspace, tmp_path, size, scale):
    native.io.display_size = size
    imgui.get_style().font_scale_main = scale
    m = workspace.manager
    draw = widgets.panels(workspace)
    workspace._tab = "Install"
    native.warm(draw)
    native.choose_next("Installation destination", draw)
    base_id = workspace._install_overlay
    source = Path(m.rows_by_id[base_id].source)
    before = {p: p.read_bytes() for p in source.rglob("*") if p.is_file()}
    incoming = tmp_path / "owned update"
    incoming.mkdir()
    (incoming / "new.txt").write_bytes(b"generated additive payload")
    workspace._prepare(incoming)
    widgets.finish_job(workspace, native, draw)
    assert m.prepared.overlay_review.base_id == base_id
    native.click(native.find("Review destination files"), draw)
    widgets.finish_job(workspace, native, draw)
    native.click(native.find("Install into library"), draw)
    widgets.finish_job(workspace, native, draw)
    assert Path(m.rows_by_id[m.selected_id].source, "new.txt").read_bytes() == b"generated additive payload"
    assert {p: p.read_bytes() for p in before} == before


@pytest.mark.parametrize("size,scale", [((1280, 800), 1.5), ((1600, 900), 1.0)])
def test_inert_member_preview_uses_actual_input_and_keeps_package_unchanged(native, workspace, tmp_path, size, scale):
    native.io.display_size = size
    imgui.get_style().font_scale_main = scale
    root = tmp_path / "preview"
    root.mkdir()
    (root / "readme.txt").write_bytes(b"<script>generated inert text</script>")
    draw = widgets.panels(workspace)
    workspace._prepare(root)
    widgets.finish_job(workspace, native, draw)
    native.click(native.find("Additional package sources"), draw)
    for _ in range(12):
        if any(item.label == "readme.txt###package_source_file" for item in native.items):
            break
        native.scroll(native.find("Additional package sources"), draw, -2)
    native.click(native.find("readme.txt###package_source_file"), draw)
    for _ in range(12):
        if any(item.label == "Preview selected package file" for item in native.items):
            break
        native.scroll(native.find("readme.txt###package_source_file"), draw, -2)
    native.click(native.find("Preview selected package file"), draw)
    widgets.finish_job(workspace, native, draw)
    assert workspace._package_preview.result.text == "<script>generated inert text</script>"
    assert (root / "readme.txt").read_bytes() == b"<script>generated inert text</script>"
    assert native.find("Review destination files").rect[3] <= size[1]
    native.click(native.find("Close installer"), draw)
    assert workspace.manager.prepared is None
